# USB Capture Method and M18 Example

## Method

1. Enumerate current interfaces/topology and target VID/PID. Do not substitute a PnP port or an old USB address for a capture address.
2. Make a short capture and verify descriptors before interpreting device traffic. For multiple matching devices, use serial identity or a controlled unplug/replug difference.
3. Control input data, device, firmware, owner, interface, endpoints, and operation sequence. Define measurement start/end precisely.
4. Reassemble protocol fragments and distinguish direction, endpoints, submit/completion pairs, lengths, and sequence information. Do not count an URB twice.
5. Separate SDK acceptance, bus submission, readback, and visual observation. A capture does not establish internal presentation completion.
6. Keep raw captures, topology, hashes, parsers, and action times in ignored local storage. Preserve invalid-target labels, and do not publish full serial numbers.

## M18 identity

Run the examples from the repository root in an elevated PowerShell with USBPcap and Wireshark/TShark installed. Npcap is not required for this USB path. Every session must locate its current interface/address and verify descriptors. Capture does not itself change configuration; device operations require their own authorized test scope.

| Property             | Reference value / selection rule                                                 |
| -------------------- | -------------------------------------------------------------------------------- |
| VID:PID              | `6603:1012`, verified in the actual descriptor                                   |
| Serial               | Read from current PnP/SDK; keep the full value locally                           |
| Parent instance      | `USB\VID_6603&PID_1012\<local-serial>`                                           |
| Vendor HID interface | Usage Page `0xFFA0`, Usage `1`; the composite device also exposes keyboard input |
| Capture interface    | Choose from current USBPcap enumeration                                          |
| USB address          | Use the current top-level tree address; recheck after reconnect                  |

`DEVPKEY_Device_Address` and `Port_#...` properties are not reliable USBPcap addresses. Other variants may have different VID/PID; use their actual descriptors rather than forcing this reference identity.

## 1. Enumerate PnP and all capture trees

```powershell
Get-PnpDevice -PresentOnly |
  Where-Object { $_.InstanceId -match '^USB\\VID_6603&PID_1012\\' } |
  Select-Object Status, FriendlyName, InstanceId

$usbPcap = 'C:\Program Files\USBPcap\USBPcapCMD.exe'
$tshark = 'C:\Program Files\Wireshark\tshark.exe'
$captureDir = Join-Path (Get-Location) '.local/reverse-engineering/captures'
New-Item -ItemType Directory -Force -Path $captureDir | Out-Null
$stamp = Get-Date -Format 'yyyyMMdd_HHmmss'
$interfacesFile = Join-Path $captureDir "interfaces_$stamp.txt"
Start-Process -FilePath $usbPcap -ArgumentList @('--extcap-interfaces') `
  -Wait -WindowStyle Hidden -RedirectStandardOutput $interfacesFile

$interfaces = @(Get-Content -LiteralPath $interfacesFile | ForEach-Object {
  if ($_ -match '^interface \{value=([^}]+)\}') { $Matches[1] }
})
if ($interfaces.Count -eq 0) { throw 'No USBPcap interfaces; check installation and driver state' }
foreach ($interface in $interfaces) {
  $name = ($interface -split '\\')[-1]
  $treeFile = Join-Path $captureDir "${name}_$stamp.txt"
  Start-Process -FilePath $usbPcap `
    -ArgumentList @('--extcap-interface', $interface, '--extcap-config') `
    -Wait -WindowStyle Hidden -RedirectStandardOutput $treeFile
  Write-Output "Device tree: $treeFile"
  Get-Content -LiteralPath $treeFile
}
```

Use the top-level device address for `--devices`, not a composite child interface ID. If names are ambiguous, save all trees, unplug the M18 itself when authorized, enumerate the difference, reconnect, then enumerate again and use the new address. Device Manager's connection view can help identify parent/serial. A generic Composite Device name or the last device left after excluding keyboards/Bluetooth is not sufficient.

## 2. Verify descriptors in a short capture

Fill values from the current enumeration rather than copying old numbers:

```powershell
$captureInterface = Read-Host 'Located USBPcap interface, for example \\.\USBPcap1'
if ($captureInterface -notin $interfaces) { throw 'Interface was not enumerated in this session' }
[int]$usbAddress = Read-Host 'Current top-level USB device address'
if ($usbAddress -lt 1 -or $usbAddress -gt 127) { throw 'USB address out of range' }
$identityFile = Join-Path $captureDir "identity_$stamp.pcap"
$captureProcess = Start-Process -FilePath $usbPcap -PassThru -WindowStyle Hidden `
  -ArgumentList @('-d', $captureInterface, '-o', ('"{0}"' -f $identityFile),
    '--devices', "$usbAddress", '--inject-descriptors')
Start-Sleep -Seconds 3
if ($captureProcess.HasExited) { throw 'Capture exited early; check permissions and interface' }
Stop-Process -Id $captureProcess.Id
$captureProcess.WaitForExit()

$identityRows = @(& $tshark -r $identityFile -Y 'usb.idVendor' -T fields `
  -e usb.bus_id -e usb.device_address -e usb.idVendor -e usb.idProduct)
if ($LASTEXITCODE -ne 0) { throw 'Cannot parse identity capture' }
$identityRows
$targetRows = @($identityRows | Where-Object {
  $fields = $_ -split "`t"
  $fields.Count -eq 4 -and $fields[1] -eq "$usbAddress"
})
if ($targetRows.Count -eq 0) { throw 'No target descriptors; do not proceed' }
foreach ($row in $targetRows) {
  $fields = $row -split "`t"
  if ($fields[2] -ne '0x6603' -or $fields[3] -ne '0x1012') {
    throw "Wrong device identity: $row"
  }
}
Write-Output "M18 confirmed: $captureInterface / address $usbAddress"
```

Columns are bus, address, VID, PID. `--inject-descriptors` obtains descriptors for an already connected device. Empty output, the wrong device, a header-only file, or a corrupt capture must be diagnosed before interpreting missing traffic. Multiple matching M18 devices still require serial/unplug-difference identity. Adjust expected IDs only for a deliberately identified different model.

## 3. Capture operations

Keep the same verified connection and exactly one device owner: official software, Desktop Alchemy, or its same-backend diagnostic. If an ownership change re-enumerates the device, repeat identity checks. USBPcap observes traffic without becoming the controller.

```powershell
$captureFile = Join-Path $captureDir "m18_pages_$stamp.pcap"
$captureProcess = Start-Process -FilePath $usbPcap -PassThru -WindowStyle Hidden `
  -ArgumentList @('-d', $captureInterface, '-o', ('"{0}"' -f $captureFile),
    '--devices', "$usbAddress", '--inject-descriptors')
Start-Sleep -Seconds 1
if ($captureProcess.HasExited) { throw 'Capture did not start' }
# Perform the authorized page operations and record their times before stopping.
```

Stop only the capture process created for this session:

```powershell
Stop-Process -Id $captureProcess.Id
$captureProcess.WaitForExit()
& 'C:\Program Files\Wireshark\capinfos.exe' -c -u $captureFile
& $tshark -r $captureFile -Y 'usb.idVendor' -T fields `
  -e usb.bus_id -e usb.device_address -e usb.idVendor -e usb.idProduct
& $tshark -r $captureFile -Y "usb.device_address == $usbAddress && usb.capdata" `
  -T fields -e frame.number -e frame.time_relative -e usb.endpoint_address |
  Select-Object -First 10
```

Verify descriptors again and require target traffic during the operation; descriptors alone do not establish a captured page switch. Do not capture an entire root hub with `-A` or all new devices to compensate for an outdated address. Reconnect/address changes require a new segment and identity check.

Use visibly distinct pages, approximately two seconds apart, and return to the original page. Performance comparison requires identical images/encoded byte counts/actions and an explicit SDK commit, app/firmware versions, interface, address, identity, action times, and capture hash.

## 4. Interpretation

- Keep raw pcap, interface trees, descriptor output, and times under ignored `.local/reverse-engineering/`; update only durable interface constraints in source docs.
- Filter/group by bus, address, direction, and endpoint. Exclude injected descriptors and duplicate submit/completion counting.
- Inspect outbound page submissions: CRT/BAT image headers, fragments, and the CRT/STP refresh boundary. Distinguish complete static pages from GIF updates; count images, lengths, and refresh markers per batch.
- Measure from first BAT USB submission through final STP submission. Do not label this completed firmware rendering time. Physical switching still requires observation.
- A final refresh does not establish atomic firmware presentation. Device identity always comes from the current topology/descriptors and, if needed, a controlled reconnection difference.

See [M18](README.md) for the current adapter contract.
