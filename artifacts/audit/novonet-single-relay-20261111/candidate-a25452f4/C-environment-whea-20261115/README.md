# C environment: corrected hardware errors found

Read-only collection on 2026-11-15. Fault host is validator-C, not a separate
fourth machine. User reports A ran 24 hours without the C symptom; this is
user-reported evidence, not a verified equal-binary/equal-load comparison.
Fetched origin/main at 405b0dff; no newer A receipt appeared in this fetch.

C: Intel Core i9-14900HX, 24 cores/32 logical processors; Lenovo LNVNB161216,
BIOS N0CN22WW (reported date 2024-04-19). Two 16 GiB modules, Samsung and
Micron, each reporting 5600 configured clock and SMBIOS type 34. Mixed vendors
are a comparison variable, not proof of instability. XMP/undervolt/temperature
and CPU microcode were not established by these inventory queries.
Windows CIM caption is Windows 11 Home Chinese, version 10.0.26300; registry
UBR 9974, DisplayVersion 26H2. Legacy registry ProductName says Windows 10:
do not use that string to contradict the live CIM caption. HypervisorPresent
is true; DeviceGuard reports VBS status 2 and running services [2].

## New direct evidence

System WHEA-Logger event 19 at Beijing 2026-11-13 20:05:18 and 20:23:01,
record IDs 110868 / 110869. Both messages report processor-core Corrected
Machine Check, Internal parity error, APIC ID 33. Raw XML is retained locally;
public projection omits machine identifiers and raw hardware payload.
Query since Nov12 succeeded; System log oldest retained event is Apr19.
This updates the earlier narrower-time no-WHEA observation. No memory
Diagnostic-Results event was returned; this does not establish a passed test.

These two events lie inside the previously recorded PID39992 TTD run
(19:19:58–20:48:17 Beijing Nov13). That run ended in the recorder module.
Temporal overlap does not prove TTD caused a hardware error, nor that these
corrected events caused any original NOVOVM RBP corruption. APIC ID 33 must
not be equated to Windows CPU index 33 or the earlier PT CPU 9.

## Next comparison boundary

Hardware/system investigation now has direct evidence and proceeds alongside
software investigation. A's 24-hour observation does not exonerate software.
Compare actual EXE/AOEM hashes, configuration, uptime, work counts, query
activity, account and topology before treating A as a matched control. D specs
in conversation remain unverified here. No remote node operation was made.
Do not change BIOS/voltage, drivers, VBS, memory configuration or reboot on
this evidence alone. Controlled hardware testing and configuration changes
must be separately scoped. Isolated BASE remains unstarted; original DB,
identity and dumps preserved; no PT, speculative repair or double relay.
