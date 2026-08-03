$ErrorActionPreference = "Stop"

$outDir = "C:\Users\liang\Desktop\闲鱼管理台"
$bin = "xianyu-manager.exe"
$static = "static"

Write-Host "=== 1/3 构建前端 ===" -ForegroundColor Cyan
Push-Location web
npm install
npm run build
Pop-Location

Write-Host "=== 2/3 构建后端（release）===" -ForegroundColor Cyan
cargo build --release

Write-Host "=== 3/3 打包到 $outDir ===" -ForegroundColor Cyan
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

Copy-Item -Force "target/release/$bin" "$outDir/$bin"

if (Test-Path "$outDir/$static") { Remove-Item -Recurse -Force "$outDir/$static" }
Copy-Item -Recurse $static "$outDir/$static"

$dataDir = "$outDir/data"
if (-not (Test-Path $dataDir)) { New-Item -ItemType Directory $dataDir | Out-Null }

Write-Host "打包完成，输出目录：$outDir" -ForegroundColor Green
Write-Host "  $bin" -ForegroundColor Green
Write-Host "  $static/" -ForegroundColor Green
Write-Host "  data/" -ForegroundColor Green
Write-Host ""
Write-Host "启动方式：cd '$outDir'; .\$bin" -ForegroundColor Yellow
