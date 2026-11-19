# Install or update Blip on Windows. Paste this in PowerShell:
#   irm https://raw.githubusercontent.com/safder2000/blip/main/install-win.ps1 | iex
# No admin rights needed. Downloads the latest setup from GitHub, checks it isn't damaged,
# installs it quietly for this user and starts Blip.
& {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'      # the progress bar makes downloads much slower in PowerShell 5
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor 3072   # TLS 1.2

    Write-Host 'Finding the latest Blip...'
    $rel = Invoke-RestMethod 'https://api.github.com/repos/safder2000/blip/releases/latest' -Headers @{ 'User-Agent' = 'Blip-installer' }
    $exe = $rel.assets | Where-Object { $_.name -like 'BlipSetup-*.exe' } | Select-Object -First 1
    $sha = $rel.assets | Where-Object { $_.name -eq "$($exe.name).sha256" } | Select-Object -First 1
    if (-not $exe -or -not $sha) { Write-Host 'The latest release has no Windows setup yet - try again in a few minutes.'; return }

    $dir = Join-Path $env:TEMP 'blip-install'
    New-Item -ItemType Directory -Force $dir | Out-Null
    $path = Join-Path $dir $exe.name
    Write-Host "Downloading Blip $($rel.tag_name)..."
    Invoke-WebRequest $exe.browser_download_url -OutFile $path -UseBasicParsing
    Invoke-WebRequest $sha.browser_download_url -OutFile "$path.sha256" -UseBasicParsing
    $want = ((Get-Content "$path.sha256" -Raw).Trim() -split '\s+')[0].ToLower()
    $got = (Get-FileHash $path -Algorithm SHA256).Hash.ToLower()
    if ($got -ne $want) { Remove-Item $path -Force; Write-Host 'The download looks damaged - please try again.'; return }

    Write-Host 'Installing...'
    # -Wait would also wait for Blip itself (the setup starts it), so wait for the setup process only
    $p = Start-Process $path -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/CLOSEAPPLICATIONS' -PassThru
    $null = $p.Handle
    $p.WaitForExit()
    Remove-Item $path, "$path.sha256" -Force -ErrorAction SilentlyContinue
    if ($p.ExitCode -ne 0) { Write-Host "Setup stopped with code $($p.ExitCode)."; return }
    Write-Host 'Blip is installed and running. It keeps itself up to date from now on.'
}
