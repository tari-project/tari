echo ""
echo ""
echo ""
echo ""
echo ""
echo "OpenSSL download page: '$env:openssl_downloads'"
$url = $env:openssl_repo + ((Invoke-WebRequest "$env:openssl_downloads" -UseBasicParsing).Links.href | `
    Select-String -Pattern 'Win64' | Select-String -Pattern 'Light-1_1' | Select-String -Pattern 'exe')
echo "OpenSSL install file:  '$url'"
Invoke-WebRequest "$url" -outfile "$env:TEMP\$env:openssl_install_file"
