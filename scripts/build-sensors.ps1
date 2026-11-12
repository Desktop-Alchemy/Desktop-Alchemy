param([switch]$Release)
$ErrorActionPreference = 'Stop'
function Get-Sha256([string]$Path) {
    $stream = [System.IO.File]::OpenRead($Path)
    try {
        $algorithm = [System.Security.Cryptography.SHA256]::Create()
        try {
            return [System.BitConverter]::ToString($algorithm.ComputeHash($stream)).Replace('-', '').ToLowerInvariant()
        } finally { $algorithm.Dispose() }
    } finally { $stream.Dispose() }
}
if (-not ('M18Build.AuthenticodeVerifier' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

namespace M18Build {
    public static class AuthenticodeVerifier {
        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct FileInfo {
            public UInt32 Size;
            public IntPtr Path;
            public IntPtr Handle;
            public IntPtr KnownSubject;
        }

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct TrustData {
            public UInt32 Size;
            public IntPtr PolicyCallbackData;
            public IntPtr SipClientData;
            public UInt32 UiChoice;
            public UInt32 RevocationChecks;
            public UInt32 UnionChoice;
            public IntPtr FileInfo;
            public UInt32 StateAction;
            public IntPtr StateData;
            public IntPtr UrlReference;
            public UInt32 ProviderFlags;
            public UInt32 UiContext;
            public IntPtr SignatureSettings;
        }

        [DllImport("wintrust.dll", ExactSpelling = true, PreserveSig = true)]
        private static extern Int32 WinVerifyTrust(IntPtr window, ref Guid action, IntPtr data);

        public static bool IsValid(string path) {
            IntPtr pathPointer = Marshal.StringToHGlobalUni(path);
            IntPtr filePointer = IntPtr.Zero;
            IntPtr dataPointer = IntPtr.Zero;
            try {
                FileInfo file = new FileInfo();
                file.Size = (UInt32)Marshal.SizeOf(typeof(FileInfo));
                file.Path = pathPointer;
                filePointer = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(FileInfo)));
                Marshal.StructureToPtr(file, filePointer, false);

                TrustData data = new TrustData();
                data.Size = (UInt32)Marshal.SizeOf(typeof(TrustData));
                data.UiChoice = 2; // WTD_UI_NONE
                data.UnionChoice = 1; // WTD_CHOICE_FILE
                data.FileInfo = filePointer;
                data.ProviderFlags = 0x1000; // WTD_CACHE_ONLY_URL_RETRIEVAL
                dataPointer = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(TrustData)));
                Marshal.StructureToPtr(data, dataPointer, false);

                Guid verifyAction = new Guid("00AAC56B-CD44-11D0-8CC2-00C04FC295EE");
                return WinVerifyTrust(IntPtr.Zero, ref verifyAction, dataPointer) == 0;
            } finally {
                if (dataPointer != IntPtr.Zero) Marshal.FreeHGlobal(dataPointer);
                if (filePointer != IntPtr.Zero) Marshal.FreeHGlobal(filePointer);
                Marshal.FreeHGlobal(pathPointer);
            }
        }
    }
}
'@
}
$taskRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$source = Join-Path $taskRoot 'plugins/performance/sensors'
$publish = Join-Path $source 'publish'
$configuration = if ($Release) { 'Release' } else { 'Debug' }
& dotnet publish (Join-Path $source 'M18.Sensors.csproj') -c $configuration -o $publish -p:RestoreLockedMode=true
if ($LASTEXITCODE -ne 0) { throw 'Sensor component build failed' }
$manifest = Get-Content -LiteralPath (Join-Path $source 'SOURCES.json') -Raw | ConvertFrom-Json
$driverDirectory = Join-Path $publish 'driver'
New-Item -ItemType Directory -Path $driverDirectory -Force | Out-Null
$driver = Join-Path $driverDirectory 'PawnIO_setup.exe'
if (!(Test-Path -LiteralPath $driver) -or (Get-Sha256 $driver) -ne $manifest.driver.sha256) {
    Invoke-WebRequest -Uri $manifest.driver.url -OutFile $driver
}
if ((Get-Sha256 $driver) -ne $manifest.driver.sha256) { throw 'PawnIO hash mismatch' }
if (-not [M18Build.AuthenticodeVerifier]::IsValid($driver)) { throw 'PawnIO signature is not valid' }
Copy-Item -LiteralPath (Join-Path $source 'SOURCES.json') -Destination $publish
$licenses = Join-Path $publish 'licenses'
New-Item -ItemType Directory -Path $licenses -Force | Out-Null
$licenseSources = @{
    'LibreHardwareMonitor-MPL-2.0.txt' = 'LibreHardwareMonitor-MPL-2.0.txt'
    'LibreHardwareMonitor-THIRD-PARTY-NOTICES.txt' = 'LibreHardwareMonitor-THIRD-PARTY-NOTICES.txt'
    'PawnIO-GPL-2.0.txt' = 'PawnIO-GPL-2.0.txt'
    'PawnIO-license-and-exception.txt' = 'PawnIO-license-and-exception.txt'
}
foreach ($entry in $licenseSources.GetEnumerator()) {
    $destination = Join-Path $licenses $entry.Key
    Copy-Item -LiteralPath (Join-Path $taskRoot "licenses/$($entry.Value)") -Destination $destination -Force
}
& (Join-Path $publish 'm18-sensors.exe') --self-test
if ($LASTEXITCODE -ne 0) { throw 'Sensor component self-test failed' }
$files = @{}
Get-ChildItem -LiteralPath $publish -Recurse -File | Where-Object Name -ne 'FILES.json' | ForEach-Object {
    $relative = $_.FullName.Substring($publish.Length + 1).Replace('\', '/')
    $files[$relative] = @{ size = $_.Length; sha256 = Get-Sha256 $_.FullName }
}
@{ files = $files } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $publish 'FILES.json') -Encoding UTF8
