## Objectif du projet

Créer un dispositif matériel type Rubber Ducky, orienté vers l'automatisation du déploiement/configuration de postes informatiques : envoi automatique de séquences de touches clavier au branchement sur un PC. Projet d'ingénierie combinant conception PCB (KiCad) et firmware embarqué en Rust.

## Contraintes générales

- Format allongé façon clé USB classique (pas carré), le plus miniaturisé possible.
- Soudable soi-même **au fer à souder uniquement** (pas de four à refusion ni de pâte à souder disponibles) : composants SMD 0603/0805 acceptables, packages à pitch fin (QFN, SON) évités.
- Budget cible : **moins de 25€** par unité.
- Le PCB final est destiné à être intégré dans un **boîtier fermé** (potentiellement à fermeture aimantée). Seuls les éléments explicitement sortis du boîtier restent accessibles une fois assemblé.
- Doit pouvoir être flashé/mis à jour facilement, idéalement sans ouvrir le boîtier pour l'usage courant.

## Connectique — port USB-A unique

- **Un seul connecteur, USB-A mâle monté en surface**

## Module RP2040 retenu : Adafruit KB2040 (PID 5302)

- **Format** : 35,0 x 17,8 x 4,9mm, rectangulaire et allongé (format "Pro Micro").
- **Point décisif du choix** : contrairement à d'autres modules RP2040 castellated bon marché (RP2040-Zero, Tiny 2040, QT Py), qui câblent leurs lignes USB D+/D- uniquement vers leur connecteur USB-C intégré sans les exposer ailleurs, le KB2040 **expose D+ et D- séparément sur des pastilles castellated dédiées** — condition indispensable pour router l'USB vers notre propre connecteur USB-A.
- **8MB de flash SPI intégrée** — pas de puce flash externe nécessaire, largement suffisant pour le firmware + quelques scripts.
- **NeoPixel intégré** — présent sur le module mais **non utilisé** dans notre design (voir section LED plus bas) : une fois dans le boîtier, sa position fixe sur le petit module contraindrait trop le design du boîtier.
- **Boutons Reset et BOOT intégrés** — utiles uniquement en dernier recours pour le flashage (voir section correspondante).
- Connecteur USB-C intégré au module : **présent mais jamais câblé ni utilisé**, purement inerte dans notre design.
- **18 GPIO exposés** sur les pastilles castellated (+2 via STEMMA QT, non utilisé) — le projet n'a besoin que de 2 GPIO (bouton + switch), large marge disponible.

### Alimentation du module depuis le connecteur USB-A

- Le module a un pin **RAW** relié au régulateur 3.3V embarqué. Un jumper au dos du module ("USB+ > RAW") doit être **soudé fermé** pour autoriser jusqu'à 2A (au lieu de 500mA par défaut), nécessaire pour un usage combiné HID + Mass Storage + flash.
- Câblage : **VBUS/5V du connecteur USB-A → pin RAW**, **GND du connecteur USB-A → pin G/GND du module**. Le module est alimenté exactement comme via son propre USB-C, sans jamais l'utiliser.

## Modes de fonctionnement

- **Mode injection** : l'appareil s'énumère en HID clavier et exécute le script sélectionné.
- **Mode configuration** : l'appareil s'énumère en Mass Storage, exposant une partition dédiée aux scripts (glisser-déposer) et la possibilité de changer le script actif avec le bouton dédié.
- **Switch physique dédié** (composant sur notre PCB) pour basculer entre les deux modes, accessible depuis l'extérieur du boîtier.

## Bouton et LED — composants dédiés sur notre PCB

Puisque l'ensemble est enfermé dans un boîtier, on s'appuie sur des composants externes dédiés plutôt que sur ceux intégrés au module (invisibles/inaccessibles une fois fermé) :

- **Bouton tactile** dédié uniquement au cycle des scripts stockés (une dizaine visée) — pas de double-fonction avec le bootloader.
- **LED séparée** (plutôt que le NeoPixel du module) positionnée librement à côté du bouton et du switch, groupés sur une même face visible du boîtier. Indique le mode actif, l'index du script sélectionné, et l'état de connexion.
- Pas de bouton reset externe dédié.

## Scripts (stockage et gestion) — indépendant du firmware

- Environ **une dizaine de scripts** de type Ducky Script (ou format custom) à stocker.
- Exposés via un système de fichiers virtuel en **mode Mass Storage**, sur une partition de la flash 8MB intégrée au module, séparée de la zone firmware.
- Système de fichiers **LittleFS** (wear-leveling intégré) pour cette zone scripts.

## Mise à jour du firmware — via le port USB-A

Les lignes D+/D- du KB2040 formant un seul bus USB accessible aussi bien par le connecteur intégré que par les pastilles breakout, tout le flashage passe naturellement par notre unique port USB-A.

- **Méthode principale (sans ouvrir le boîtier)** : le firmware expose une interface **USB CDC-ACM** en plus de HID/MSC. Un logiciel compagnon (à développer en Rust) envoie une commande sur cette interface pour déclencher `reset_to_usb_boot()` (fonction ROM du RP2040), redémarrant l'appareil en bootloader UF2. Le logiciel compagnon détecte le volume UF2 apparu et y copie automatiquement le nouveau firmware.
- **Méthode de secours (dernier recours, boîtier ouvert)** : si le mécanisme logiciel échoue (premier flash, firmware corrompu), ouverture du boîtier pour accéder au bouton BOOT du module et forcer l'entrée en bootloader manuellement. Compromis assumé, cas rare.

## Architecture matérielle (résumé)

| Composant                                                                | Rôle                                                         | Notes                                                  |
| ------------------------------------------------------------------------ | ------------------------------------------------------------ | ------------------------------------------------------ |
| Module Adafruit KB2040 (PID 5302)                                        | RP2040 + 8MB flash + régulateur 3.3V + crystal, tout intégré | ~8,95€, castellated, D+/D- et RAW exposés en pastilles |
| Connecteur USB-A mâle (Würth 692112030100, horizontal)                   | Seul port de la carte : alimentation + données + flash       | Composant standard, symbole/empreinte natifs KiCad     |
| Bouton tactile                                                           | Cycle des scripts stockés                                    | THT classique, accessible depuis le boîtier            |
| Switch (slide ou tactile)                                                | Bascule mode injection / configuration                       | Accessible depuis le boîtier                           |
| LED (simple ou RGB)                                                      | Indication mode / script actif / statut                      | Composant séparé, positionné librement pour le boîtier |
| Résistances de protection USB (séries sur D+/D-, pull-down CC si besoin) | Protection/conformité électrique de la ligne USB             | Passifs simples, 0603/0805                             |

## Stack logicielle envisagée

- **Firmware** : Rust `no_std`, `embassy-rp` (HAL async), `usb-device` + `usbd-hid` + `usbd-serial` + implémentation Mass Storage (device USB composite), `littlefs2` pour la zone scripts.
- **Bootloader** : bootloader UF2 natif en ROM du RP2040 (aucun développement nécessaire).
- **Logiciel compagnon PC** (à développer, Rust) : déclenchement du reset-to-bootloader via port série, copie automatique du firmware, aide optionnelle à la gestion/validation des scripts.
