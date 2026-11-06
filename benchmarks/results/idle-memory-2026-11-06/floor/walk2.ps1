param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Database,
    [int]$Width = 1600,
    [int]$Height = 900,
    [int]$SettleSeconds = 12,
    [int]$Top = 24,
    [string]$Tag = "run"
)

$ErrorActionPreference = "Stop"

$typeSource = @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class QuireMemWalk2 {
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern IntPtr OpenProcess(int access, bool inherit, int pid);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern bool CloseHandle(IntPtr h);
    [DllImport("psapi.dll", SetLastError=true)]
    public static extern bool EnumProcessModulesEx(IntPtr h, IntPtr[] mods, uint cb, out uint needed, uint filter);
    [DllImport("psapi.dll", CharSet=CharSet.Unicode)]
    public static extern uint GetMappedFileNameW(IntPtr h, IntPtr mod, StringBuilder s, uint size);

    [StructLayout(LayoutKind.Sequential)]
    public struct MBI {
        public IntPtr BaseAddress; public IntPtr AllocationBase;
        public uint AllocationProtect; public uint __aligned1;
        public IntPtr RegionSize; public uint State; public uint Protect; public uint Type;
        public uint __aligned2;
    }
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern uint VirtualQueryEx(IntPtr h, IntPtr addr, out MBI info, uint len);
    [DllImport("psapi.dll", SetLastError=true)]
    public static extern bool QueryWorkingSet(IntPtr h, IntPtr buf, uint cb);
    [DllImport("psapi.dll", SetLastError=true)]
    public static extern bool GetProcessMemoryInfo(IntPtr h, ref PMC p, int cb);
    [StructLayout(LayoutKind.Sequential)]
    public struct PMC {
        public uint cb; public uint PageFaultCount;
        public ulong PeakWorkingSetSize; public ulong WorkingSetSize;
        public ulong QuotaPeakPagedPoolUsage; public ulong QuotaPagedPoolUsage;
        public ulong QuotaPeakNonPagedPoolUsage; public ulong QuotaNonPagedPoolUsage;
        public ulong PagefileUsage; public ulong PeakPagefileUsage; public ulong PrivateUsage;
    }

    const uint MEM_IMAGE = 0x1000000, MEM_MAPPED = 0x40000, MEM_PRIVATE = 0x20000;
    const int LIST_MODULES_ALL = 0x03;
    const int PAGE = 4096;

    public class Bucket {
        public string Label;
        public double ResidentMb;
        public double CommitMb;
        public long MinAddr = long.MaxValue;
        public long MaxAddr;
        public int MaxShare;
        public HashSet<long> AllocBases = new HashSet<long>();
    }

    public static string Walk(int pid, int top) {
        StringBuilder o = new StringBuilder();
        IntPtr h = OpenProcess(0x0410, false, pid);
        if (h == IntPtr.Zero) h = OpenProcess(0x1F0FFF, false, pid);
        if (h == IntPtr.Zero) return "OpenProcess failed err=" + Marshal.GetLastWin32Error();
        try {
            PMC pmc = new PMC();
            pmc.cb = (uint)Marshal.SizeOf(typeof(PMC));
            if (GetProcessMemoryInfo(h, ref pmc, (int)pmc.cb)) {
                o.AppendLine(string.Format("SUMMARY WS={0:F1} PeakWS={1:F1} Private={2:F1} PeakPrivate={3:F1}",
                    pmc.WorkingSetSize/1048576.0, pmc.PeakWorkingSetSize/1048576.0,
                    pmc.PrivateUsage/1048576.0, pmc.PeakPagefileUsage/1048576.0));
            }

            var nameByBase = new Dictionary<long,string>();
            uint needed;
            EnumProcessModulesEx(h, null, 0, out needed, LIST_MODULES_ALL);
            if (needed > 0) {
                IntPtr[] mods = new IntPtr[needed / (uint)IntPtr.Size];
                if (EnumProcessModulesEx(h, mods, needed, out needed, LIST_MODULES_ALL)) {
                    var sb = new StringBuilder(1024);
                    foreach (IntPtr m in mods) {
                        sb.Length = 0;
                        if (GetMappedFileNameW(h, m, sb, 1024) > 0 && !nameByBase.ContainsKey((long)m))
                            nameByBase[(long)m] = Tail(sb.ToString());
                    }
                }
            }

            IntPtr probe = Marshal.AllocHGlobal(8);
            try {
                Marshal.WriteIntPtr(probe, IntPtr.Zero);
                QueryWorkingSet(h, probe, 8);
                long count = Marshal.ReadIntPtr(probe).ToInt64();
                o.AppendLine("resident pages=" + count);
                if (count <= 0 || count > 8000000) return o.ToString();

                int bytes = 8 + (int)count * 16;
                IntPtr buf = Marshal.AllocHGlobal(bytes);
                try {
                    if (!QueryWorkingSet(h, buf, (uint)bytes))
                        return o.AppendLine("QueryWorkingSet failed err=" + Marshal.GetLastWin32Error()).ToString();

                    var buckets = new Dictionary<string, Bucket>();
                    var commitOnce = new HashSet<long>();
                    var mbiCache = new Dictionary<long, MBI>();
                    long mask = ~(long)(PAGE - 1);
                    for (long i = 0; i < count; i++) {
                        IntPtr entry = (IntPtr)(buf.ToInt64() + 8 + i * 16);
                        long va = Marshal.ReadIntPtr(entry).ToInt64() & mask;
                        ulong bits = (ulong)Marshal.ReadIntPtr((IntPtr)(entry.ToInt64() + 8)).ToInt64();
                        int share = (int)(bits & 0xFFF);

                        MBI m;
                        long alignedBase = va - (va % 0x10000);
                        if (!mbiCache.TryGetValue(alignedBase, out m)) {
                            if (VirtualQueryEx(h, (IntPtr)va, out m, (uint)Marshal.SizeOf(typeof(MBI))) == 0) continue;
                            mbiCache[alignedBase] = m;
                        }
                        string t = m.Type == MEM_IMAGE ? "IMAGE" : (m.Type == MEM_MAPPED ? "MAPPED" : "PRIVATE");
                        string nm;
                        if (t == "IMAGE") {
                            if (!nameByBase.TryGetValue((long)m.AllocationBase, out nm)) nm = "<image-unknown>";
                        } else if (t == "MAPPED") {
                            var sb = new StringBuilder(1024);
                            nm = GetMappedFileNameW(h, (IntPtr)m.AllocationBase, sb, 1024) > 0 ? Tail(sb.ToString()) : "<mapped-unknown>";
                        } else {
                            nm = "alloc@" + (long)m.AllocationBase;
                        }
                        string key = t + "|" + nm;
                        Bucket b;
                        if (!buckets.TryGetValue(key, out b)) { b = new Bucket(); b.Label = key; buckets[key] = b; }
                        b.ResidentMb += PAGE / 1048576.0;
                        if (va < b.MinAddr) b.MinAddr = va;
                        if (va > b.MaxAddr) b.MaxAddr = va;
                        if (share > b.MaxShare) b.MaxShare = share;
                        long ab = (long)m.AllocationBase;
                        if (b.AllocBases.Add(ab) && commitOnce.Add(ab)) b.CommitMb += (long)m.RegionSize / 1048576.0;
                    }

                    var list = new List<Bucket>(buckets.Values);
                    list.Sort((x, y) => y.ResidentMb.CompareTo(x.ResidentMb));
                    for (int i = 0; i < Math.Min(top, list.Count); i++) {
                        Bucket b = list[i];
                        o.AppendLine(string.Format("{0,7:F2} res | {1,8:F1} commit | {2,5} allocs | maxshare={3,3} | [{4:X}..{5:X}] | {6}",
                            b.ResidentMb, b.CommitMb, b.AllocBases.Count, b.MaxShare, b.MinAddr, b.MaxAddr, b.Label));
                    }
                    double rest = 0;
                    for (int i = top; i < list.Count; i++) rest += list[i].ResidentMb;
                    o.AppendLine(string.Format("{0,7:F2} res in {1} further buckets", rest, list.Count - top));

                    double privT = 0, imgT = 0, mapT = 0;
                    foreach (Bucket b in list) {
                        if (b.Label.StartsWith("PRIVATE")) privT += b.ResidentMb;
                        else if (b.Label.StartsWith("IMAGE")) imgT += b.ResidentMb;
                        else mapT += b.ResidentMb;
                    }
                    o.AppendLine(string.Format("BY TYPE PRIVATE={0:F1}  IMAGE={1:F1}  MAPPED={2:F1}", privT, imgT, mapT));
                    return o.ToString();
                } finally { Marshal.FreeHGlobal(buf); }
            } finally { Marshal.FreeHGlobal(probe); }
        } finally { CloseHandle(h); }
    }

    static string Tail(string s) {
        int i = s.LastIndexOf('\\');
        return i < 0 ? s : s.Substring(i + 1);
    }
}
'@

Add-Type -TypeDefinition $typeSource -Language CSharp
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class QW2 {
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int ht, bool r);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RC r);
    public struct RC { public int L, T, R, B; }
}
'@ -Language CSharp

$dbPath = [IO.Path]::GetFullPath($Database)
$exePath = (Resolve-Path -LiteralPath $Exe).Path
$process = Start-Process -FilePath $exePath -ArgumentList @("--db", $dbPath) -PassThru
$handle = [IntPtr]::Zero
$sw = [Diagnostics.Stopwatch]::StartNew()
while ($handle -eq [IntPtr]::Zero -and $sw.Elapsed.TotalSeconds -lt 25) {
    Start-Sleep -Milliseconds 20
    $live = Get-Process -Id $process.Id -ErrorAction SilentlyContinue
    if (-not $live) { break }
    $handle = $live.MainWindowHandle
}
if ($handle -eq [IntPtr]::Zero) { Stop-Process -Id $process.Id -Force; throw "no window appeared" }
[void][QW2]::ShowWindow($handle, 9)
[void][QW2]::MoveWindow($handle, 40, 40, $Width, $Height, $true)
$rect = New-Object QW2+RC
[void][QW2]::GetWindowRect($handle, [ref]$rect)
$actualW = $rect.R - $rect.L
$actualH = $rect.B - $rect.T
Start-Sleep -Seconds $SettleSeconds
$live2 = Get-Process -Id $process.Id
Write-Host ("=== $Tag  window " + $actualW + "x" + $actualH + "  after " + $SettleSeconds + "s settle ===")
$result = [QuireMemWalk2]::Walk($process.Id, $Top)
Write-Host $result
Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
$out = Join-Path $PSScriptRoot ("walk2-" + $Tag + ".txt")
("TAG=$Tag window=${actualW}x${actualH}`r`n" + $result) | Set-Content -LiteralPath $out -Encoding UTF8
Write-Host ("saved " + $out)
