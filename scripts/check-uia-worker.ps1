# Exercise the actual Edge fixture query helper without starting a browser.
$ErrorActionPreference = 'Stop'
if ($env:OS -eq 'Windows_NT') {
    Add-Type -AssemblyName System.Windows.Forms
} else {
    # PowerShell Core on Linux can test pipeline completion, but has no WinForms.
    Add-Type 'namespace System.Windows.Forms { public static class Application { public static void DoEvents() {} } }'
}
$fixture = Join-Path $PSScriptRoot '../crates/sister-capture/tests/fixtures/uia-edge-reader.ps1'
$tokens = $null
$errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($fixture, [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw "Edge fixture does not parse: $errors" }
$helpers = @($ast.FindAll({
    param($node)
    $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Invoke-SisterUiaQuery'
}, $true))
if ($helpers.Count -ne 1) { throw 'Expected exactly one Edge query helper' }
Invoke-Expression $helpers[0].Extent.Text

$state = [System.Management.Automation.Runspaces.InitialSessionState]::CreateDefault()
$state.Commands.Add([System.Management.Automation.Runspaces.SessionStateFunctionEntry]::new('Read-FixturePage', @'
param([string]$Text)
Start-Sleep -Milliseconds 30
[pscustomobject]@{ Text = $Text; Apartment = [Threading.Thread]::CurrentThread.GetApartmentState().ToString() }
'@))
$state.Commands.Add([System.Management.Automation.Runspaces.SessionStateFunctionEntry]::new('Test-FixtureFocus', 'return $false'))
$state.Commands.Add([System.Management.Automation.Runspaces.SessionStateFunctionEntry]::new('Read-FixtureError', 'throw "fixture-provider-error"'))
$uiaRunspace = [System.Management.Automation.Runspaces.RunspaceFactory]::CreateRunspace($state)
$uiaRunspace.ApartmentState = [Threading.ApartmentState]::MTA
$uiaRunspace.ThreadOptions = [System.Management.Automation.Runspaces.PSThreadOptions]::ReuseThread
$uiaRunspace.Open()
$script:SisterUiaPending = $null
try {
    $page = Invoke-SisterUiaQuery 'Read-FixturePage' @{ Text = 'worker-result' }
    if ($page.Text -cne 'worker-result') { throw 'Query lost its arguments or result' }
    if ($env:OS -eq 'Windows_NT' -and $page.Apartment -ne 'MTA') { throw 'Query did not execute on MTA' }
    $focused = Invoke-SisterUiaQuery 'Test-FixtureFocus'
    if ($focused -isnot [bool] -or $focused) { throw 'False focus result changed type or value' }
    $caught = $null
    try { Invoke-SisterUiaQuery 'Read-FixtureError' | Out-Null } catch { $caught = $_ }
    if ($null -eq $caught -or $caught.ToString() -notlike '*fixture-provider-error*') {
        throw 'Provider failure was lost or replaced by a timeout'
    }
    $again = Invoke-SisterUiaQuery 'Read-FixturePage' @{ Text = 'after-error' }
    if ($again.Text -cne 'after-error') { throw 'Worker could not be reused after a provider failure' }
    Write-Host 'Edge UIA worker: results, arguments, false, errors and reuse passed.'
} catch {
    # A regressed async pipeline can keep the process alive even after exit.
    # Fail this isolated check immediately; it owns no browser or user data.
    [Console]::Error.WriteLine($_.ToString())
    [Environment]::Exit(1)
} finally {
    $uiaRunspace.Dispose()
}
