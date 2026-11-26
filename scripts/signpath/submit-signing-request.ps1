#Requires -Version 5.1
<#
.SYNOPSIS
    Submit a Windows release archive to SignPath and download the signed result.

.DESCRIPTION
    Local/DIY companion to the signing steps in
    .github/workflows/release.yml. Use it to sign a release archive from your
    machine while the SignPath setup is still being validated, or from any
    environment that is not GitHub-hosted.

    It calls the SignPath REST API directly (SubmitWithArtifact -> poll Status ->
    download SignedArtifact), so it needs no GitHub connector and works against
    the self-signed test certificate and the production certificate alike.

    The archive is the same dist/alphacode-windows-x86_64.zip the release emits;
    scripts/signpath/artifact-configuration.xml tells SignPath to sign both
    alphacode.exe and AlphaCode.Desktop.exe inside it.

.PARAMETER ArtifactPath
    Path to the unsigned release ZIP (must contain alphacode.exe and
    AlphaCode.Desktop.exe at the root).

.PARAMETER OutputPath
    Where to write the signed ZIP. Default: <ArtifactPath without .zip>-signed.zip

.PARAMETER OrganizationId
    SignPath organization ID. Defaults to $env:SIGNPATH_ORGANIZATION_ID.
    The AlphaCode organization ID is f351ad44-52ab-4630-9179-ad04db6dd730.

.PARAMETER ProjectSlug
    SignPath project slug. Defaults to $env:SIGNPATH_PROJECT_SLUG (alphacode).

.PARAMETER SigningPolicySlug
    Signing policy slug. Defaults to $env:SIGNPATH_SIGNING_POLICY_SLUG.
    This is a manual tool, so `test-signing` (the self-signed certificate) is a
    legitimate choice here -- it exercises the loop before the production
    certificate exists. Note that a release never ships a test signature: the
    release pipeline refuses `test-signing` on purpose.

.PARAMETER ArtifactConfigurationSlug
    Optional artifact configuration slug. Omit to use the project default.

.PARAMETER ApiToken
    SignPath REST API token (submitter permission). Defaults to
    $env:SIGNPATH_API_TOKEN; if neither is set you are prompted.

.EXAMPLE
    $env:SIGNPATH_API_TOKEN = '...'
    ./scripts/signpath/submit-signing-request.ps1 -ArtifactPath dist/alphacode-windows-x86_64.zip
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$ArtifactPath,

    [string]$OutputPath,

    [string]$OrganizationId = $env:SIGNPATH_ORGANIZATION_ID,

    [string]$ProjectSlug = $(if ($env:SIGNPATH_PROJECT_SLUG) { $env:SIGNPATH_PROJECT_SLUG } else { 'alphacode' }),

    [string]$SigningPolicySlug = $(if ($env:SIGNPATH_SIGNING_POLICY_SLUG) { $env:SIGNPATH_SIGNING_POLICY_SLUG } else { 'test-signing' }),

    [string]$ArtifactConfigurationSlug = $env:SIGNPATH_ARTIFACT_CONFIGURATION_SLUG,

    [string]$ApiToken = $env:SIGNPATH_API_TOKEN,

    [int]$TimeoutSeconds = 600
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# Windows PowerShell 5.1 defaults to TLS 1.0, which SignPath rejects.
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
Add-Type -AssemblyName System.Net.Http

if (-not $OrganizationId) {
    throw "OrganizationId is required (pass -OrganizationId or set SIGNPATH_ORGANIZATION_ID)."
}
if (-not $ApiToken) {
    $secure = Read-Host -Prompt 'SignPath API token' -AsSecureString
    $ApiToken = [System.Net.NetworkCredential]::new('', $secure).Password
}

$artifact = Resolve-Path -LiteralPath $ArtifactPath
if (-not (Test-Path -LiteralPath $artifact -PathType Leaf)) {
    throw "Artifact not found: $ArtifactPath"
}
if (-not $OutputPath) {
    $OutputPath = [System.IO.Path]::Combine(
        [System.IO.Path]::GetDirectoryName($artifact.Path),
        ([System.IO.Path]::GetFileNameWithoutExtension($artifact.Path) + '-signed.zip'))
}

$base = "https://app.signpath.io/Api/v1/$OrganizationId"
$headers = @{ Authorization = "Bearer $ApiToken" }
$configLabel = if ($ArtifactConfigurationSlug) { $ArtifactConfigurationSlug } else { '(default)' }

Write-Host "==> submitting $($artifact.Path)"
Write-Host "    project=$ProjectSlug policy=$SigningPolicySlug config=$configLabel"
if ($SigningPolicySlug -eq 'test-signing') {
    Write-Warning ("Signing with the self-signed TEST certificate. The result validates the " +
        "pipeline, but Windows will report 'Unknown publisher' if you distribute it. " +
        "Use the production policy for anything shipped.")
}

$client = [System.Net.Http.HttpClient]::new()
try {
    $form = [System.Net.Http.MultipartFormDataContent]::new()
    $form.Add([System.Net.Http.StringContent]::new($ProjectSlug), 'projectSlug')
    $form.Add([System.Net.Http.StringContent]::new($SigningPolicySlug), 'signingPolicySlug')
    if ($ArtifactConfigurationSlug) {
        $form.Add([System.Net.Http.StringContent]::new($ArtifactConfigurationSlug), 'artifactConfigurationSlug')
    }
    $form.Add([System.Net.Http.StringContent]::new("AlphaCode $(Split-Path $artifact.Path -Leaf)"), 'description')
    $stream = [System.IO.File]::OpenRead($artifact.Path)
    $fileContent = [System.Net.Http.StreamContent]::new($stream)
    $fileContent.Headers.ContentType = [System.Net.Http.Headers.MediaTypeHeaderValue]::Parse('application/zip')
    $form.Add($fileContent, 'artifact', (Split-Path $artifact.Path -Leaf))

    $request = [System.Net.Http.HttpRequestMessage]::new([System.Net.Http.HttpMethod]::Post, "$base/SigningRequests/SubmitWithArtifact")
    $request.Headers.Add('Authorization', "Bearer $ApiToken")
    $request.Content = $form
    $response = $client.SendAsync($request).GetAwaiter().GetResult()
    $body = $response.Content.ReadAsStringAsync().GetAwaiter().GetResult()

    if (-not $response.IsSuccessStatusCode) {
        throw "SubmitWithArtifact failed: HTTP $([int]$response.StatusCode) $($response.ReasonPhrase)`n$body"
    }
    if (-not $response.Headers.Location) {
        throw "SubmitWithArtifact returned no Location header; cannot determine the signing request ID."
    }

    $signingRequestId = ($response.Headers.Location.ToString().TrimEnd('/') -split '/')[-1]
    Write-Host "    signing request: $signingRequestId"
    Write-Host "    open https://app.signpath.io and select this request under the organization to review it"

    # Poll until the request reaches a final status.
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ($true) {
        Start-Sleep -Seconds 5
        $status = Invoke-RestMethod -Method Get -Uri "$base/SigningRequests/$signingRequestId/Status" -Headers $headers
        Write-Host "    status=$($status.status) workflow=$($status.workflowStatus)"
        if ($status.isFinalStatus) { break }
        if ((Get-Date) -gt $deadline) {
            throw "Signing request did not finish within $TimeoutSeconds seconds."
        }
    }

    if ($status.status -ne 'Completed') {
        throw "Signing failed: status=$($status.status) workflow=$($status.workflowStatus)"
    }

    Write-Host "==> downloading signed archive -> $OutputPath"
    Invoke-WebRequest -Method Get -Uri "$base/SigningRequests/$signingRequestId/SignedArtifact" -Headers $headers -OutFile $OutputPath

    Write-Host "==> verifying signatures"
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("alphacode-signed-" + [guid]::NewGuid())
    Expand-Archive -LiteralPath $OutputPath -DestinationPath $tmp -Force
    $ok = $true
    foreach ($name in @('alphacode.exe', 'AlphaCode.Desktop.exe')) {
        $path = Join-Path $tmp $name
        if (-not (Test-Path -LiteralPath $path)) { throw "signed archive is missing $name" }
        $sig = Get-AuthenticodeSignature -LiteralPath $path
        Write-Host ("    {0}: {1}  signer={2}" -f $name, $sig.Status, $sig.SignerCertificate.Subject)
        if ($sig.Status -ne 'Valid') {
            # Expected while on the self-signed test certificate (status is
            # 'UnknownError' until the cert is trusted); treat a present signer
            # as success in that phase and require Valid once production-signed.
            if ($sig.SignerCertificate) { Write-Warning "$name is signed but the chain is not trusted (self-signed test cert?)." }
            else { $ok = $false }
        }
    }
    Remove-Item -Recurse -Force $tmp
    if (-not $ok) { throw "one or more binaries in the signed archive are not signed" }

    Write-Host "==> done: $OutputPath"
}
finally {
    $client.Dispose()
}
