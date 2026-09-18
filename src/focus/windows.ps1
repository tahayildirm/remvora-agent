$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
# Metadata only: never read ValuePattern.Value or TextPattern text.
$element = [System.Windows.Automation.AutomationElement]::FocusedElement
$result = $null
if ($null -ne $element -and $element.Current.IsEnabled -and -not $element.Current.IsOffscreen) {
    $pattern = $null
    $editable = $element.Current.ControlType -eq [System.Windows.Automation.ControlType]::Edit
    if ($element.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
        $editable = -not $pattern.Current.IsReadOnly
    }
    if ($editable) {
        $r = $element.Current.BoundingRectangle
        $result = @{x=$r.X; y=$r.Y; width=$r.Width; height=$r.Height}
    }
}
ConvertTo-Json -InputObject $result -Compress
