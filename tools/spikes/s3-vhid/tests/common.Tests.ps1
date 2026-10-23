# Pester 3.4 (in-box) tests for the pure helpers in common.ps1.
. (Join-Path (Split-Path $PSScriptRoot -Parent) 'common.ps1')

Describe 'Get-S3CollectionKind' {
    It 'maps Col01 to keyboard' { Get-S3CollectionKind @('HID\VID_DEED&PID_FEED&REV_0101&Col01', 'HID\VID_DEED&PID_FEED&Col01') | Should Be 'keyboard' }
    It 'maps Col02 to mouse-rel' { Get-S3CollectionKind @('HID\VID_DEED&PID_FEED&Col02') | Should Be 'mouse-rel' }
    It 'maps Col03 to mouse-abs' { Get-S3CollectionKind @('HID\VID_DEED&PID_FEED&Col03') | Should Be 'mouse-abs' }
    It 'maps Col04 to gamepad' { Get-S3CollectionKind @('HID\VID_DEED&PID_FEED&Col04') | Should Be 'gamepad' }
    It 'maps Col05 to control' { Get-S3CollectionKind @('hid\vid_deed&pid_feed&col05') | Should Be 'control' }
    It 'maps the root-enumerated form (parent hardware ID)' {
        Get-S3CollectionKind @('HID\MixLinkVHid&Col01', 'HID\VID_DEED&UP:0001_U:0006', 'HID_DEVICE') | Should Be 'keyboard'
    }
    It 'maps root-enumerated Col04 to gamepad' { Get-S3CollectionKind @('HID\MixLinkVHid&Col04') | Should Be 'gamepad' }
    It 'returns other for the parent device itself' { Get-S3CollectionKind @('Root\MixLinkVHid') | Should Be 'other' }
    It 'returns other for a foreign device' { Get-S3CollectionKind @('HID\VID_046D&PID_C52B&Col01') | Should Be 'other' }
    It 'returns other for no IDs' { Get-S3CollectionKind @() | Should Be 'other' }
}

Describe 'Test-S3ExpectedCollections' {
    It 'passes with one of each kind' {
        Test-S3ExpectedCollections @('keyboard', 'mouse-rel', 'mouse-abs', 'gamepad', 'control') | Should Be $true
    }
    It 'fails when the gamepad is missing' {
        Test-S3ExpectedCollections @('keyboard', 'mouse-rel', 'mouse-abs', 'control') | Should Be $false
    }
    It 'fails on a duplicate' {
        Test-S3ExpectedCollections @('keyboard', 'keyboard', 'mouse-rel', 'mouse-abs', 'gamepad', 'control') | Should Be $false
    }
}
