# Pester 3.4 (in-box) tests for the pure helpers in common.ps1.
. (Join-Path (Split-Path $PSScriptRoot -Parent) 'common.ps1')

Describe 'ConvertTo-S1SacState' {
    It 'maps 0 to Off' { ConvertTo-S1SacState 0 | Should Be 'Off' }
    It 'maps 1 to On' { ConvertTo-S1SacState 1 | Should Be 'On' }
    It 'maps 2 to Evaluation' { ConvertTo-S1SacState 2 | Should Be 'Evaluation' }
    It 'maps anything else to Unknown' { ConvertTo-S1SacState 7 | Should Be 'Unknown' }
}

Describe 'Test-S1TestSigning' {
    It 'is true when bcdedit says Yes' {
        Test-S1TestSigning @('identifier              {current}', 'testsigning             Yes') | Should Be $true
    }
    It 'is false when bcdedit says No' {
        Test-S1TestSigning @('testsigning             No') | Should Be $false
    }
    It 'is false when the line is absent' {
        Test-S1TestSigning @() | Should Be $false
    }
}
