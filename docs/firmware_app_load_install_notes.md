# Firmware App Loading and Installation

Native firmware owns installed SQBC apps in the internal LittleFS application
store. The registry, persistent app state, lifecycle checkpoints, and OTA
metadata are internal-flash data; books and general content remain on SD.

Install paths validate the complete SQBC payload before atomic publication.
Temporary runs execute from bounded RAM-backed storage and remain development
oriented. Foreground launch, exit, return, armed timers, and armed logical input
events use the native runtime lifecycle described in
`docs/firmware_state_machines.md`.

The device protocol and `squidc app install|launch|list` commands are the public
host workflow. Firmware storage geometry is target-specific and must not leak
into the SquidScript language contract.
