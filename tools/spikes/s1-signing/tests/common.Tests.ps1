# Pester 3.4 (in-box) tests for the pure helpers in common.ps1.
. (Join-Path (Split-Path $PSScriptRoot -Parent) 'common.ps1')

Describe 'ConvertTo-S1SacState' {
    It 'maps 0 to Off' { ConvertTo-S1SacState 0 | Should Be 'Off' }
    It 'maps 1 to On' { ConvertTo-S1SacState 1 | Should Be 'On' }
    It 'maps 2 to Evaluation' { ConvertTo-S1SacState 2 | Should Be 'Evaluation' }
    It 'maps anything else to Unknown' { ConvertTo-S1SacState 7 | Should Be 'Unknown' }
}

Describe 'ConvertFrom-S1CertutilKeyInfo' {
    $lines = @(
        '  Key Container = {F76D93E8-9F99-4DD0-9580-01D6D1793484}',
        '  Unique container name: aabcb4a11dab4c48b40afae0e05716a2_2d01d75b-ec67-4191-9a03-bde354b794ba',
        '  Provider = Microsoft Software Key Storage Provider')
    $info = ConvertFrom-S1CertutilKeyInfo $lines
    It 'reads the container' { $info.Container | Should Be '{F76D93E8-9F99-4DD0-9580-01D6D1793484}' }
    It 'reads the unique name' { $info.UniqueName | Should Be 'aabcb4a11dab4c48b40afae0e05716a2_2d01d75b-ec67-4191-9a03-bde354b794ba' }
    It 'reads the provider' { $info.Provider | Should Be 'Microsoft Software Key Storage Provider' }
    It 'leaves fields empty when absent' { (ConvertFrom-S1CertutilKeyInfo @('nothing')).Container | Should BeNullOrEmpty }
}

Describe 'Test-S1MachineKey' {
    It 'is null when no key name is known' {
        Test-S1MachineKey ([pscustomobject]@{ Container = $null; UniqueName = $null; Provider = $null }) | Should BeNullOrEmpty
    }
    It 'is false for a key that does not exist' {
        $info = [pscustomobject]@{ Container = 'mixlink-s1-no-such-key'; UniqueName = 'no_such_file'; Provider = $null }
        Test-S1MachineKey $info | Should Be $false
    }
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
