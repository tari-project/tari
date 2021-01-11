echo "XMRig repo: '$env:xmrig_repo'"
$url = (Invoke-WebRequest "$env:xmrig_repo" -UseBasicParsing | ConvertFrom-Json).assets.browser_download_url | `
    Select-String -Pattern 'macos-x64.tar.gz'
echo "Install from '$url'"
Invoke-WebRequest "$url" -outfile "/tmp/$env:xmrig_zip"
echo "Downloaded  '$url'"
