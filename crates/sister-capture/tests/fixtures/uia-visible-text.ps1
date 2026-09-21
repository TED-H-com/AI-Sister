# Owned, synthetic WPF provider for windows_uia.rs. No real user app is automated.
param([Parameter(Mandatory=$true)][string]$StateDir)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName PresentationFramework
. (Join-Path $PSScriptRoot 'uia-backdrop.ps1')
$backdrop = New-SisterUiaBackdrop

$window = New-Object System.Windows.Window
$window.Title = 'AI-Sister UIA native fixture'
$window.Width = 600
$window.Height = 340
$window.Left = 40
$window.Top = 40
$window.Topmost = $true
$window.WindowStartupLocation = 'Manual'

$edit = New-Object System.Windows.Controls.TextBox
$edit.FontSize = 18
$edit.AcceptsReturn = $true
$edit.VerticalScrollBarVisibility = 'Auto'
$edit.Margin = '16'
$edit.Text = "$([char]0x96fb)$([char]0x8a71) 0800-123-456`r`nvisible@example.test" + ("`r`npadding" * 120) + "`r`nOFFSCREEN-SENTINEL"
$window.Content = $edit

# RichTextBox exposes the native Document control type and TextPattern. Keep it
# read-only; scrolling changes the visible range without changing the document.
$document = New-Object System.Windows.Controls.RichTextBox
$document.IsReadOnly = $true
$document.FontSize = 18
$document.Margin = '16'
$document.VerticalScrollBarVisibility = 'Auto'
$lines = @("$([char]0x6587)$([char]0x4ef6) 0800-222-333", "$([char]0x672c)$([char]0x671f)$([char]0x61c9)$([char]0x7e73)$([char]0x91d1)$([char]0x984d)", 'DOCUMENT-SECOND-PARAGRAPH') + (1..120 | ForEach-Object { "padding $_" }) + @('DOCUMENT-BOTTOM 02-9988-7766')
foreach ($line in $lines) {
    $paragraph = New-Object System.Windows.Documents.Paragraph
    $paragraph.Inlines.Add((New-Object System.Windows.Documents.Run -ArgumentList $line))
    $document.Document.Blocks.Add($paragraph)
}

$password = New-Object System.Windows.Controls.PasswordBox
$password.Password = 'PASSWORD-SENTINEL'
$password.Margin = '16'
$password.Height = 60
$button = New-Object System.Windows.Controls.Button
$button.Content = 'BUTTON-SENTINEL'
$button.Margin = '16'

$other = New-Object System.Windows.Window
$other.Title = 'AI-Sister UIA other window'
$other.Width = 600
$other.Height = 200
$other.Left = 60
$other.Top = 60
$other.Topmost = $true
$otherEdit = New-Object System.Windows.Controls.TextBox
$otherEdit.Text = 'OTHER-WINDOW-SENTINEL'
$otherEdit.Margin = '16'
$other.Content = $otherEdit

$script:last = ''
$script:pending = ''
$script:activeWindow = $window
$script:activeControl = $edit
$deadline = [DateTime]::UtcNow.AddSeconds(90)
$timer = New-Object System.Windows.Threading.DispatcherTimer
$timer.Interval = [TimeSpan]::FromMilliseconds(40)
$timer.Add_Tick({
    try {
        if ([DateTime]::UtcNow -ge $deadline) {
            $timer.Stop()
            $other.Close()
            $window.Close()
            return
        }
        $path = Join-Path $StateDir 'request'
        if (-not [IO.File]::Exists($path)) { return }
        $mode = [IO.File]::ReadAllText($path)
        if ($mode -ne $script:pending -and $mode -ne $script:last) {
            switch ($mode) {
                'stop' { $timer.Stop(); $window.Close(); return }
                'edit' { $edit.ScrollToHome(); $script:activeControl = $edit }
                'changed' { $edit.Text = 'CHANGED-SENTINEL 02-2233-4455'; $script:activeControl = $edit }
                'document' {
                    $window.Content = $document
                    $document.CaretPosition = $document.Document.ContentStart
                    $document.ScrollToHome()
                    $script:activeControl = $document
                }
                'document-scrolled' {
                    $document.CaretPosition = $document.Document.ContentEnd
                    $document.ScrollToEnd()
                    $script:activeControl = $document
                }
                'password' { $window.Content = $password; $script:activeControl = $password }
                'button' { $window.Content = $button; $script:activeControl = $button }
                'other' { $other.Show(); $script:activeWindow = $other; $script:activeControl = $otherEdit }
                default { return }
            }
            $script:pending = $mode
        }
        if ($script:pending -ne $script:last) {
            $script:activeWindow.Activate() | Out-Null
            $script:activeControl.Focus() | Out-Null
            $script:activeWindow.UpdateLayout()
            if ($script:activeControl.IsKeyboardFocused) {
                [IO.File]::WriteAllText((Join-Path $StateDir 'ready'), $script:pending)
                $script:last = $script:pending
            }
        }
    } catch {
        [IO.File]::WriteAllText((Join-Path $StateDir 'error'), $_.Exception.ToString())
        $timer.Stop()
        $other.Close()
        $window.Close()
    }
})
$window.Add_Closed({ $timer.Stop(); $other.Close() })
$timer.Start()
try { $window.ShowDialog() | Out-Null } finally { $backdrop.Dispose() }
