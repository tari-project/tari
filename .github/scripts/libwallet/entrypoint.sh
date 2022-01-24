#!/bin/bash
PLATFORMS=$1
LEVEL=$2
SRCDIR=$3
VERSION=$4
echo "VERSION=${VERSION}"

# fix version for branches and tags
case $VERSION in

*"heads"*)
    VERSION=${VERSION/refs\/heads\//}
    echo "Parsed BRANCH from git ref: ${VERSION}"
    ;;

*"tags"*)
    VERSION=${VERSION/refs\/tags\/libwallet-/}
    echo "Parsed TAG from git ref: ${VERSION}"
    ;;

*)
    echo "Failed to parse a version from ${VERSION}"
    exit 1
    ;;
esac

echo "Action Build Libs: Invoking with"
echo "PLATFORMS: ${PLATFORMS}"
echo "LEVEL: ${LEVEL}"
echo "SRCDIR: ${SRCDIR}"
echo "VERSION: ${VERSION}"

IFS=';' read -ra PLATFORMARRAY <<<"$PLATFORMS"

for platform in "${PLATFORMARRAY[@]}"; do
    .github/scripts/libwallet/build_jnilib.sh "${platform}" "${LEVEL}" "${SRCDIR}" || exit 1
done

.github/scripts/libwallet/hash_libs.sh "$PLATFORMS" "$VERSION" "${SRCDIR}"
