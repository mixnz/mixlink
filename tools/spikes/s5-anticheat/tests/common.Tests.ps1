# Pester 3.4 (in-box) tests for the pure helpers in common.ps1.
. (Join-Path (Split-Path $PSScriptRoot -Parent) 'common.ps1')

Describe 'S5 games' {
    It 'lists Apex then Elden Ring' { ($S5Games | ForEach-Object Key) -join ',' | Should Be 'apex,elden' }
}

Describe 'Test-S5AcServiceName' {
    $apex = Get-S5Game 'apex'
    $elden = Get-S5Game 'elden'
    It 'Apex: matches EAAntiCheatService' { Test-S5AcServiceName $apex 'EAAntiCheatService' | Should Be $true }
    It 'Apex: ignores case' { Test-S5AcServiceName $apex 'eaanticheatservice' | Should Be $true }
    It 'Apex: rejects a longer name' { Test-S5AcServiceName $apex 'EAAntiCheatServiceX' | Should Be $false }
    It 'Apex: rejects EAC' { Test-S5AcServiceName $apex 'EasyAntiCheat_EOS' | Should Be $false }
    It 'Elden Ring: matches EasyAntiCheat_EOS' { Test-S5AcServiceName $elden 'EasyAntiCheat_EOS' | Should Be $true }
    It 'Elden Ring: matches EasyAntiCheat' { Test-S5AcServiceName $elden 'EasyAntiCheat' | Should Be $true }
    It 'Elden Ring: rejects Javelin' { Test-S5AcServiceName $elden 'EAAntiCheatService' | Should Be $false }
    It 'rejects empty' { Test-S5AcServiceName $apex '' | Should Be $false }
    It 'Get-S5Game throws on an unknown key' { { Get-S5Game 'nope' } | Should Throw }
}

Describe 'ConvertTo-S5MixLinkState' {
    It 'is clean with no reasons' { ConvertTo-S5MixLinkState @() | Should Be 'clean' }
    It 'is unknown when only not-checked reasons' {
        ConvertTo-S5MixLinkState @('driver packages not checked (not elevated)') | Should Be 'unknown'
    }
    It 'is installed when any real reason' {
        ConvertTo-S5MixLinkState @('driver packages not checked (not elevated)', 'device ROOT\DISPLAY\0000') | Should Be 'installed'
    }
    It 'is installed with a certificate left' { ConvertTo-S5MixLinkState @('certificate AB12 in Root') | Should Be 'installed' }
}

Describe 'Select-S5NewestLogs' {
    It 'returns nothing for a missing dir' { @(Select-S5NewestLogs 'TestDrive:\nope' 5).Count | Should Be 0 }
    It 'returns the newest log or txt file only' {
        $dir = Join-Path $TestDrive 'logs'
        New-Item -ItemType Directory -Force (Join-Path $dir 'sub') | Out-Null
        $old = New-Item -ItemType File (Join-Path $dir 'a.log')
        $new = New-Item -ItemType File (Join-Path $dir 'sub\b.txt')
        $bin = New-Item -ItemType File (Join-Path $dir 'c.bin')
        $old.LastWriteTime = (Get-Date).AddHours(-2)
        $new.LastWriteTime = (Get-Date).AddHours(-1)
        $bin.LastWriteTime = Get-Date
        $picked = @(Select-S5NewestLogs $dir 1)
        $picked.Count | Should Be 1
        $picked[0].Name | Should Be 'b.txt'
    }
}

Describe 'Get-S5NextStep' {
    It 'starts with A' { Get-S5NextStep '' | Should Be 'A' }
    It 'installs after A' { Get-S5NextStep 'A' | Should Be 'install' }
    It 'reboots after install' { Get-S5NextStep 'install' | Should Be 'reboot' }
    It 'runs B after reboot' { Get-S5NextStep 'reboot' | Should Be 'B' }
    It 'runs C after B' { Get-S5NextStep 'B' | Should Be 'C' }
    It 'runs D after C' { Get-S5NextStep 'C' | Should Be 'D' }
    It 'is done after D' { Get-S5NextStep 'D' | Should Be 'done' }
    It 'stays done' { Get-S5NextStep 'done' | Should Be 'done' }
    It 'throws on an unknown step' { { Get-S5NextStep 'X' } | Should Throw }
}

Describe 'ConvertTo-S5LaunchVerdict' {
    It 'passes when everything held' { ConvertTo-S5LaunchVerdict $true 300 300 $true $true | Should Be 'pass' }
    It 'fails when the game never started' { ConvertTo-S5LaunchVerdict $false 0 300 $false $false | Should Be 'fail: game did not start' }
    It 'fails when the game exited early' { ConvertTo-S5LaunchVerdict $true 42 300 $true $true | Should Be 'fail: game exited after 42 s' }
    It 'fails when the anti-cheat never ran' { ConvertTo-S5LaunchVerdict $true 300 300 $false $true | Should Be 'fail: anti-cheat service never seen running' }
    It 'fails when the user saw no menu' { ConvertTo-S5LaunchVerdict $true 300 300 $true $false | Should Be 'fail: main menu not reached' }
}

Describe 'Test-S5VirtualAdapterId' {
    It 'matches the root display instance' { Test-S5VirtualAdapterId 'ROOT\DISPLAY\0000' | Should Be $true }
    It 'matches our hardware ID' { Test-S5VirtualAdapterId 'Root\MixLinkVDisplay' | Should Be $true }
    It 'rejects a PCI GPU' { Test-S5VirtualAdapterId 'PCI\VEN_10DE&DEV_2F04' | Should Be $false }
    It 'rejects empty' { Test-S5VirtualAdapterId '' | Should Be $false }
}
