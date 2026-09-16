/* Adafruit KB2040: RP2040 with 8 MiB of QSPI flash (W25Q64) and 264 KiB of SRAM.
 *
 * Only the first 1 MiB of flash is given to the linker. The rest is managed by the
 * firmware itself (see storage/layout.rs): a FAT volume exposed over USB and a
 * settings sector at the very end.
 */
MEMORY {
    BOOT2 : ORIGIN = 0x10000000, LENGTH = 0x100
    FLASH : ORIGIN = 0x10000100, LENGTH = 1024K - 0x100
    RAM   : ORIGIN = 0x20000000, LENGTH = 256K
}

EXTERN(BOOT2_FIRMWARE)

SECTIONS {
    /* Second stage bootloader, must be the first 256 bytes of flash. */
    .boot2 ORIGIN(BOOT2) :
    {
        KEEP(*(.boot2));
    } > BOOT2
} INSERT BEFORE .text;
