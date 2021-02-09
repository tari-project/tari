echo "XMRig repo: '$env:xmrig_repo'"
# TODO: Standardize on version 6.6.2 for now as later version(s) has a breaking interface change with the merge mining proxy
#$url = (Invoke-WebRequest "$env:xmrig_repo" -UseBasicParsing | ConvertFrom-Json).assets.browser_download_url | `
#    Select-String -Pattern 'linux-x64.tar.gz'
$url = (Invoke-WebRequest "$env:xmrig_repo" -UseBasicParsing | ConvertFrom-Json).assets.browser_download_url | `
    Select-String -Pattern '6.6.2-linux-x64.tar.gz'
echo "Install from '$url'"
Invoke-WebRequest "$url" -outfile "/tmp/$env:xmrig_zip"
echo "Downloaded  '$url'"
