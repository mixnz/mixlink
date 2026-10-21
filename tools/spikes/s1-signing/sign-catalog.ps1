# Catalog signing for spike S1. Dot-source only. ASCII only.
# Method 1: Set-AuthenticodeSignature. Method 2 (fallback): SignerSignEx3 from mssign32.dll.

$S1SignerSource = @'
using System;
using System.Runtime.InteropServices;

public static class S1Signer
{
    [StructLayout(LayoutKind.Sequential)]
    struct SIGNER_FILE_INFO { public uint cbSize; [MarshalAs(UnmanagedType.LPWStr)] public string pwszFileName; public IntPtr hFile; }

    [StructLayout(LayoutKind.Sequential)]
    struct SIGNER_SUBJECT_INFO { public uint cbSize; public IntPtr pdwIndex; public uint dwSubjectChoice; public IntPtr pSignerFileInfo; }

    [StructLayout(LayoutKind.Sequential)]
    struct SIGNER_CERT_STORE_INFO { public uint cbSize; public IntPtr pSigningCert; public uint dwCertPolicy; public IntPtr hCertStore; }

    [StructLayout(LayoutKind.Sequential)]
    struct SIGNER_CERT { public uint cbSize; public uint dwCertChoice; public IntPtr pCertStoreInfo; public IntPtr hwnd; }

    [StructLayout(LayoutKind.Sequential)]
    struct SIGNER_SIGNATURE_INFO { public uint cbSize; public uint algidHash; public uint dwAttrChoice; public IntPtr pAttrAuthcode; public IntPtr psAuthenticated; public IntPtr psUnauthenticated; }

    [DllImport("mssign32.dll", CharSet = CharSet.Unicode)]
    static extern int SignerSignEx3(uint dwFlags, IntPtr pSubjectInfo, IntPtr pSignerCert, IntPtr pSignatureInfo,
        IntPtr pProviderInfo, uint dwTimestampFlags, IntPtr pszTimestampAlgorithmOid, IntPtr pwszHttpTimeStamp,
        IntPtr psRequest, IntPtr pSipData, out IntPtr ppSignerContext, IntPtr pCryptoPolicy,
        IntPtr pDigestSignInfo, IntPtr pReserved);

    [DllImport("mssign32.dll")]
    static extern int SignerFreeSignerContext(IntPtr pSignerContext);

    const uint SIGNER_SUBJECT_FILE = 1;
    const uint SIGNER_CERT_STORE = 2;
    const uint SIGNER_CERT_POLICY_CHAIN = 2;
    const uint CALG_SHA_256 = 0x800c;
    const uint SIGNER_NO_ATTR = 0;

    static IntPtr Alloc<T>(T value) where T : struct
    {
        IntPtr p = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(T)));
        Marshal.StructureToPtr(value, p, false);
        return p;
    }

    // Signs the file in place with the certificate context (PCCERT_CONTEXT); returns the HRESULT.
    public static int Sign(string path, IntPtr certContext)
    {
        IntPtr index = Marshal.AllocHGlobal(4);
        Marshal.WriteInt32(index, 0);
        IntPtr fileInfo = Alloc(new SIGNER_FILE_INFO { cbSize = (uint)Marshal.SizeOf(typeof(SIGNER_FILE_INFO)), pwszFileName = path });
        IntPtr subject = Alloc(new SIGNER_SUBJECT_INFO { cbSize = (uint)Marshal.SizeOf(typeof(SIGNER_SUBJECT_INFO)), pdwIndex = index, dwSubjectChoice = SIGNER_SUBJECT_FILE, pSignerFileInfo = fileInfo });
        IntPtr storeInfo = Alloc(new SIGNER_CERT_STORE_INFO { cbSize = (uint)Marshal.SizeOf(typeof(SIGNER_CERT_STORE_INFO)), pSigningCert = certContext, dwCertPolicy = SIGNER_CERT_POLICY_CHAIN });
        IntPtr cert = Alloc(new SIGNER_CERT { cbSize = (uint)Marshal.SizeOf(typeof(SIGNER_CERT)), dwCertChoice = SIGNER_CERT_STORE, pCertStoreInfo = storeInfo });
        IntPtr sigInfo = Alloc(new SIGNER_SIGNATURE_INFO { cbSize = (uint)Marshal.SizeOf(typeof(SIGNER_SIGNATURE_INFO)), algidHash = CALG_SHA_256, dwAttrChoice = SIGNER_NO_ATTR });
        IntPtr context = IntPtr.Zero;
        try
        {
            return SignerSignEx3(0, subject, cert, sigInfo, IntPtr.Zero, 0, IntPtr.Zero, IntPtr.Zero,
                IntPtr.Zero, IntPtr.Zero, out context, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero);
        }
        finally
        {
            if (context != IntPtr.Zero) SignerFreeSignerContext(context);
            Marshal.DestroyStructure(fileInfo, typeof(SIGNER_FILE_INFO));
            foreach (IntPtr p in new[] { index, fileInfo, subject, storeInfo, cert, sigInfo }) Marshal.FreeHGlobal(p);
        }
    }
}
'@

function Test-S1SignedBy([string] $Path, [System.Security.Cryptography.X509Certificates.X509Certificate2] $Cert) {
    $sig = Get-AuthenticodeSignature $Path
    return ($null -ne $sig.SignerCertificate -and $sig.SignerCertificate.Thumbprint -eq $Cert.Thumbprint)
}

# Signs the catalog in place; returns the method that worked. Throws when both fail.
function Invoke-S1CatalogSign([string] $CatPath, [System.Security.Cryptography.X509Certificates.X509Certificate2] $Cert) {
    try {
        $result = Set-AuthenticodeSignature -FilePath $CatPath -Certificate $Cert -HashAlgorithm SHA256 -ErrorAction Stop
        Write-Host "Set-AuthenticodeSignature: status $($result.Status), message: $($result.StatusMessage)"
    } catch { Write-Host "Set-AuthenticodeSignature threw: $($_.Exception.Message)" }
    if (Test-S1SignedBy $CatPath $Cert) { return 'Set-AuthenticodeSignature' }

    Write-Host 'Falling back to SignerSignEx3.'
    if (-not ('S1Signer' -as [type])) { Add-Type -TypeDefinition $S1SignerSource -Language CSharp }
    $hr = [S1Signer]::Sign($CatPath, $Cert.Handle)
    Write-Host ("SignerSignEx3: HRESULT 0x{0:X8}" -f $hr)
    if ($hr -eq 0 -and (Test-S1SignedBy $CatPath $Cert)) { return 'SignerSignEx3' }
    throw "Catalog signing failed with both methods: $CatPath"
}
