# Build folders for parallel sessions (CONTRIBUTING's "Building in parallel"):
#
#   scripts/target.ps1 new <name>     target/<name>, started from the seed's
#                                     dependencies and warmed in the background
#   scripts/target.ps1 remove <name>  stops its warm build and deletes it
#   scripts/target.ps1 seed [rev]     rebuilds target/seed from main (or rev)
#
# The seed hands over dependencies only. Cargo trusts file times for
# Rook's own crates, so their outputs from another checkout could pass for
# fresh against older sources; registry sources never change, and Cargo
# still checks every dependency it is handed.
param(
    [Parameter(Mandatory)][ValidateSet("new", "remove", "seed", "warm")][string]$Command,
    [string]$Name
)
$ErrorActionPreference = "Stop"
$checkout = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
# Every worktree shares the main checkout's target folder.
$targets = Join-Path (Split-Path (git -C $checkout rev-parse --path-format=absolute --git-common-dir)) "target"
$seed = Join-Path $targets "seed"

# Compiles what `scripts/check` compiles, so a warm folder has all of it;
# returns the step that failed, if one did.
function Build-Gate([string]$log) {
    $ErrorActionPreference = "Continue"
    $steps = @(
        @("clippy", "--workspace", "--all-targets"),
        @("test", "--workspace", "--no-run"),
        @("build", "--workspace", "--bin", "rook-tools"))
    foreach ($step in $steps) {
        if ($log) { cargo @step *>> $log } else { cargo @step | Out-Host }
        if ($LASTEXITCODE -ne 0) { return "cargo $($step -join ' ')" }
    }
}

function Copy-Folder($from, $to, [string[]]$exclude) {
    $skip = if ($exclude) { @("/XD") + $exclude + @("/XF") + $exclude } else { @() }
    # Robocopy keeps file times, which Cargo's freshness checks rely on.
    robocopy $from $to /E /MT:16 /NFL /NDL /NJH /NJS /NP @skip | Out-Null
    if ($LASTEXITCODE -ge 8) { throw "copying $from to $to failed (robocopy $LASTEXITCODE)" }
}

switch ($Command) {
    "new" {
        if (-not $Name) { throw "usage: scripts/target.ps1 new <name>" }
        $dir = Join-Path $targets $Name
        if (Test-Path $dir) { throw "$dir already exists" }
        if (Test-Path $seed) {
            try {
                Copy-Folder $seed $dir @("rook*", "librook*", "cargo-timings", "seed.txt")
            } catch {
                Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
                throw
            }
            $built = Get-Content (Join-Path $seed "seed.txt") -ErrorAction SilentlyContinue
            if ($built -ne (Get-FileHash (Join-Path $checkout "Cargo.lock")).Hash) {
                Write-Host "The seed was built from another Cargo.lock; changed dependencies build here."
            }
        } else {
            Write-Host "No seed yet (scripts/target.ps1 seed): $Name starts empty."
        }
        New-Item -ItemType Directory -Force $dir | Out-Null
        $shell = (Get-Process -Id $PID).Path
        $warm = Start-Process $shell -WindowStyle Hidden -PassThru -ArgumentList @(
            "-NoProfile", "-File", "`"$PSCommandPath`"", "warm", $Name)
        Set-Content (Join-Path $dir "warm.pid") $warm.Id
        Write-Host "CARGO_TARGET_DIR=$dir"
        Write-Host "Warming in the background: $(Join-Path $dir 'warm.log')"
    }
    "warm" {
        # Below normal, so the session's own builds and the user's apps come
        # first; the compilers it starts inherit the priority.
        (Get-Process -Id $PID).PriorityClass = "BelowNormal"
        $dir = Join-Path $targets $Name
        $log = Join-Path $dir "warm.log"
        $env:CARGO_TARGET_DIR = $dir
        Set-Location $checkout
        $failed = Build-Gate $log
        Add-Content $log $(if ($failed) { "warm: $failed failed" } else { "warm: done" })
        Remove-Item (Join-Path $dir "warm.pid") -ErrorAction SilentlyContinue
    }
    "remove" {
        if (-not $Name -or $Name -eq "seed") { throw "usage: scripts/target.ps1 remove <name>" }
        $dir = Join-Path $targets $Name
        $pidFile = Join-Path $dir "warm.pid"
        if (Test-Path $pidFile) {
            $warm = Get-Process -Id (Get-Content $pidFile) -ErrorAction SilentlyContinue
            if ($warm -and $warm.Path -eq (Get-Process -Id $PID).Path) {
                taskkill /PID $warm.Id /T /F | Out-Null
            }
        }
        if (Test-Path $dir) { Remove-Item -Recurse -Force $dir }
    }
    "seed" {
        # Built from main in a worktree of its own: a worktree inside this
        # checkout reads both copies of .cargo/config.toml and passes their
        # rustflags twice, so a seed built in the main tree wouldn't match.
        $tree = Join-Path (Split-Path $targets) ".claude/worktrees/seed"
        $revision = if ($Name) { $Name } else { "main" }
        git -C $checkout worktree add --detach $tree $revision
        if ($LASTEXITCODE -ne 0) { throw "seed: can't make $tree" }
        # Built beside the seed and swapped in, so a `new` copying the seed
        # meanwhile gets the whole old one or fails, never a mix.
        $next = Join-Path $targets "seed-next"
        try {
            if ((Test-Path $seed) -and -not (Test-Path $next)) { Copy-Folder $seed $next }
            $env:CARGO_TARGET_DIR = $next
            Set-Location $tree
            $failed = Build-Gate
            $lock = (Get-FileHash (Join-Path $tree "Cargo.lock")).Hash
        } finally {
            Set-Location $checkout
            git -C $checkout worktree remove --force $tree
        }
        if ($failed) { throw "seed: $failed failed; target/seed is unchanged" }
        Set-Content (Join-Path $next "seed.txt") $lock
        $old = Join-Path $targets "seed-old"
        if (Test-Path $seed) { Rename-Item $seed $old }
        Rename-Item $next $seed
        if (Test-Path $old) { Remove-Item -Recurse -Force $old }
        Write-Host "target/seed rebuilt"
    }
}
