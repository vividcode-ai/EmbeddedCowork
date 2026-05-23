param(
  [Parameter(Mandatory = $true)]
  [string]$FilePath
)

if (-not $env:GITHUB_ACTIONS) {
    Write-Host "Not in CI, skipping signing"
    exit 0
}

$required = @(
    "AZURE_CLIENT_ID",
    "AZURE_TENANT_ID",
    "AZURE_SUBSCRIPTION_ID",
    "AZURE_TRUSTED_SIGNING_ACCOUNT_NAME",
    "AZURE_TRUSTED_SIGNING_CERTIFICATE_PROFILE",
    "AZURE_TRUSTED_SIGNING_ENDPOINT"
)
foreach ($var in $required) {
    if (-not (Get-Item "env:$var" -ErrorAction SilentlyContinue)) {
        Write-Host "$var not set, skipping signing"
        exit 0
    }
}

Write-Host "Signing: $FilePath"

Install-Module -Name TrustedSigning -Scope CurrentUser -Force -AllowClobber -SkipPublisherCheck

Invoke-TrustedSigning -FilePath $FilePath `
    -CertificateProfileName $env:AZURE_TRUSTED_SIGNING_CERTIFICATE_PROFILE `
    -Endpoint $env:AZURE_TRUSTED_SIGNING_ENDPOINT `
    -RawSign

Write-Host "Signing completed: $FilePath"
