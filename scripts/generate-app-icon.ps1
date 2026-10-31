param([string]$OutputDirectory = (Join-Path $PSScriptRoot '../assets/app-icons'))
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName PresentationFramework, PresentationCore, WindowsBase
[xml]$markup = @'
<Canvas xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Width="512" Height="512">
 <Rectangle Canvas.Left="12" Canvas.Top="12" Width="488" Height="488" RadiusX="132" RadiusY="132"><Rectangle.Fill><LinearGradientBrush StartPoint="0,0" EndPoint="1,1"><GradientStop Color="#3A2F20"/><GradientStop Color="#171511" Offset="1"/></LinearGradientBrush></Rectangle.Fill></Rectangle>
 <Path Data="M256 63 444 181 400 376 256 461 112 376 68 181Z" Fill="#2B241A" StrokeThickness="13" StrokeLineJoin="Round"><Path.Stroke><LinearGradientBrush StartPoint="0,0" EndPoint=".8,1"><GradientStop Color="#F7D88C"/><GradientStop Color="#D7A93F" Offset=".55"/><GradientStop Color="#A77424" Offset="1"/></LinearGradientBrush></Path.Stroke></Path>
 <Path Data="M154 342 256 139 358 342 M190 276 322 276" StrokeThickness="30" StrokeStartLineCap="Round" StrokeEndLineCap="Round" StrokeLineJoin="Round"><Path.Stroke><LinearGradientBrush StartPoint="0,0" EndPoint=".8,1"><GradientStop Color="#F7D88C"/><GradientStop Color="#D7A93F" Offset=".55"/><GradientStop Color="#A77424" Offset="1"/></LinearGradientBrush></Path.Stroke></Path>
 <Path Data="M239 328 262 350 239 372 M287 372 314 372" Stroke="#FFF0C9" StrokeThickness="12" StrokeStartLineCap="Round" StrokeEndLineCap="Round" StrokeLineJoin="Round"/>
</Canvas>
'@
$visual = [Windows.Markup.XamlReader]::Load((New-Object Xml.XmlNodeReader $markup))
$visual.Measure([Windows.Size]::new(512,512)); $visual.Arrange([Windows.Rect]::new(0,0,512,512)); $visual.UpdateLayout()
[IO.Directory]::CreateDirectory([IO.Path]::GetFullPath($OutputDirectory)) | Out-Null
$frames = @()
foreach ($size in @(16,24,32,48,64,128,256,512)) {
 $bitmap = [Windows.Media.Imaging.RenderTargetBitmap]::new($size,$size,96*$size/512,96*$size/512,[Windows.Media.PixelFormats]::Pbgra32)
 $bitmap.Render($visual)
 $encoder = [Windows.Media.Imaging.PngBitmapEncoder]::new(); $encoder.Frames.Add([Windows.Media.Imaging.BitmapFrame]::Create($bitmap))
 $memory = [IO.MemoryStream]::new(); $encoder.Save($memory); $bytes = $memory.ToArray(); $memory.Dispose()
 if ($size -le 256) { $frames += [pscustomobject]@{size=$size;data=$bytes} }
 if ($size -eq 512) { [IO.File]::WriteAllBytes((Join-Path $OutputDirectory 'alphacode.png'),$bytes) }
}
$file = [IO.File]::Create((Join-Path $OutputDirectory 'alphacode.ico')); $writer = [IO.BinaryWriter]::new($file)
try {
 $writer.Write([uint16]0); $writer.Write([uint16]1); $writer.Write([uint16]$frames.Count)
 $offset = 6 + 16 * $frames.Count
 foreach ($frame in $frames) {
  $dimension = if ($frame.size -eq 256) {0} else {$frame.size}
  $writer.Write([byte]$dimension); $writer.Write([byte]$dimension); $writer.Write([byte]0); $writer.Write([byte]0)
  $writer.Write([uint16]1); $writer.Write([uint16]32); $writer.Write([uint32]$frame.data.Length); $writer.Write([uint32]$offset)
  $offset += $frame.data.Length
 }
 foreach ($frame in $frames) { $writer.Write([byte[]]$frame.data) }
} finally { $writer.Dispose(); $file.Dispose() }
