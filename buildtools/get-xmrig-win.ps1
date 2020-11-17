echo ""
echo ""
echo ""
echo ""
echo ""
echo "XMRig repo: '$env:xmrig_repo'"
$url = (Invoke-WebRequest "$env:xmrig_repo" | ConvertFrom-Json).assets.browser_download_url | Select-String -Pattern 'msvc-win64.zip'
echo "Install from '$url'"
Invoke-WebRequest "$url" -outfile "$env:TEMP\$env:xmrig_zip"
