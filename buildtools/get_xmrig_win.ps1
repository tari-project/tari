echo ""
echo ""
echo ""
echo ""
echo ""
echo "XMRig repo: '$env:xmrig_repo'"
# TODO: Standardize on version 6.6.2 for now as later version(s) has a breaking interface change with the merge mining proxy
#$url = (Invoke-WebRequest "$env:xmrig_repo" -UseBasicParsing | ConvertFrom-Json).assets.browser_download_url | `
#    Select-String -Pattern 'msvc-win64.zip'
$url = (Invoke-WebRequest "$env:xmrig_repo" -UseBasicParsing | ConvertFrom-Json).assets.browser_download_url | `
    Select-String -Pattern '6.6.2-msvc-win64.zip'
echo "Install from '$url'"
Invoke-WebRequest "$url" -outfile "$env:TEMP\$env:xmrig_zip"
