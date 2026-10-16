param(
    [Parameter(Mandatory = $true)]
    [string[]]$Path
)

$ErrorActionPreference = 'Stop'

if (-not ('QuireWindowsManifestProbe' -as [type])) {
    $source = @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;

public static class QuireWindowsManifestProbe {
    [DllImport("kernel32", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern IntPtr LoadLibraryEx(string file, IntPtr reserved, uint flags);
    [DllImport("kernel32", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern IntPtr FindResource(IntPtr module, IntPtr name, IntPtr type);
    [DllImport("kernel32", SetLastError = true)]
    static extern uint SizeofResource(IntPtr module, IntPtr resource);
    [DllImport("kernel32", SetLastError = true)]
    static extern IntPtr LoadResource(IntPtr module, IntPtr resource);
    [DllImport("kernel32", SetLastError = true)]
    static extern IntPtr LockResource(IntPtr resource);
    [DllImport("kernel32", SetLastError = true)]
    static extern bool FreeLibrary(IntPtr module);

    public static byte[] Read(string path) {
        var module = LoadLibraryEx(path, IntPtr.Zero, 0x2);
        if (module == IntPtr.Zero)
            throw new Win32Exception(Marshal.GetLastWin32Error());
        try {
            var resource = FindResource(module, new IntPtr(1), new IntPtr(24));
            if (resource == IntPtr.Zero)
                throw new Exception("RT_MANIFEST resource id 1 not found");
            var size = SizeofResource(module, resource);
            var handle = LoadResource(module, resource);
            var pointer = LockResource(handle);
            if (pointer == IntPtr.Zero || size == 0)
                throw new Exception("manifest resource is empty");
            var bytes = new byte[size];
            Marshal.Copy(pointer, bytes, 0, (int)size);
            return bytes;
        } finally {
            FreeLibrary(module);
        }
    }
}
'@
    Add-Type -TypeDefinition $source
}

foreach ($candidate in $Path) {
    $resolved = (Resolve-Path -LiteralPath $candidate).Path
    $manifest = [Text.Encoding]::UTF8.GetString(
        [QuireWindowsManifestProbe]::Read($resolved)
    )
    if ($manifest -notmatch '<dpiAware[^>]*>true</dpiAware>' -or
        $manifest -notmatch '<dpiAwareness[^>]*>PerMonitorV2</dpiAwareness>') {
        throw "Executable does not embed the required PerMonitorV2 manifest: $resolved"
    }
    Write-Output "DPI_MANIFEST=PerMonitorV2 PASS path=$resolved"
}
