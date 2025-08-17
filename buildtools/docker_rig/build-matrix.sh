#!/usr/bin/env bash
#
# prep json build matrix from args
#
# ./build-matrix.sh all "4.4.1" "linux/amd64,linux/arm64" mainnet
# ./build-matrix.sh minotari_sha3_miner "4.4.1" "linux/arm64" igor
# ./build-matrix.sh tor "4.4.1" "linux/arm64"
#

set -euxo pipefail

build_items=${1:-minotari_all}
echo "Building with ${build_items}."
if [ -z "${build_items}" ] || [ "${build_items}" = "minotari_all" ] ; then
  echo "Build all Minotari images"
  matrix_selection=$( jq -s -c '.[]' tarisuite.json )
elif [ "${build_items:0:9}" = "minotari_" ] ; then
  echo "Build only selected minotari images - ${build_items}"
  matrix_selection=$( jq --arg jsonVar "${build_items}" -r '[. []
    | select(."image_name"==$jsonVar)]' tarisuite.json )
elif [ "${build_items}" = "all" ] ; then
  echo "Build all images"
  matrix_selection=$(jq -s -c '.[0] + .[1]' tarisuite.json 3rdparty.json)
elif [ "${build_items:0:8}" = "3rdparty" ] ; then
  echo "Build only 3rdparty images - ${build_items}"
  matrix_selection=$( jq -s -c '.[]' 3rdparty.json )
else
  echo "Build only selected 3rdparty images - ${build_items}"
  matrix_selection=$( jq --arg jsonVar "${build_items}" -r '[. []
    | select(."image_name"==$jsonVar)]' 3rdparty.json )
fi

# Choose version prefix for minotari builds
MINOTARI_VERSION="${2:-dev}"  # e.g., pass in tag as first arg

# Start jSon string
matrix_details="["

#echo "${matrix_selection}" | jq -c '.'
while read -r item; do

  image_name=$(jq -r '.image_name' <<< "${item}")
  #echo "Image: ${image_name}"
  #echo "JSon Object: $(jq -r '.' <<< "${item}")"

  # Determine version
  if [[ "${image_name}" == minotari_* ]]; then
    version="${MINOTARI_VERSION}"
    dockerfile="tarilabs.Dockerfile"
    build_arg=""
  elif [[ -f "${image_name}.Dockerfile" ]]; then
    uppername=$(echo "${image_name}" | tr '[:lower:]' '[:upper:]')
    version=$(awk -v search="^ARG ${uppername}_VERSION=" \
      -F '=' '$0 ~ search \
        { gsub(/["]/, "", $2); print $2 }' "${image_name}.Dockerfile")
    #version+="-${MINOTARI_VERSION}"
    dockerfile="${image_name}.Dockerfile"
    build_arg="${uppername}_VERSION=${version}"
  else
    echo "No Dockerfile for ${image_name}, skipping..."
    continue
  fi

  #echo "${version}, ${dockerfile}, ${build_arg}"

  # Extend the original JSON object with new fields
  enriched=$(jq -c \
    --arg version "$version" \
    --arg dockerfile "$dockerfile" \
    --arg build_args "$build_arg" \
    '. + {
      version: $version,
      dockerfile: $dockerfile,
      build_args: $build_args
    }' <<< "${item}")

  matrix_details+="$enriched,"
done < <(jq -c '.[]' <<< "${matrix_selection}")

if [[ "${matrix_details}" == "[" ]]; then
  matrix_details="[]"  # no entries were added
  echo "!! Broken selection? !!"
  exit 1
else
  # Trim trailing comma and close string
  matrix_details="${matrix_details%,}"
  matrix_details+="]"
fi

#echo "${matrix_details}"
#echo "${matrix_details}" | jq .

build_platforms=${3:-"linux/arm64, linux/amd64"}
mapfile -t platform_list < <(echo "${build_platforms}" | tr ',' '\n'| awk '{$1=$1; print}')
# Convert platform list to JSON array
platforms_json=$(jq -n --argjson p "$(printf '%s\n' "${platform_list[@]}" | jq -R . | jq -s .)" '$p')
matrix_platforms=$(jq --argjson platforms "$platforms_json" '
  [
    .[] as $b |
    $platforms[] as $p |
    $b + {
      platform: $p,
      runner: (
        if $p | test("arm64") then "ubuntu-24.04-arm"
        else "ubuntu-latest"
        end
      ),
      arch: (
        if $p | test("arm64") then "arm64"
        else "amd64"
        end
      )
    }
  ]
' <<< "${matrix_details}")

# Prep multTest setups
build_networks=${4:-"esme, mainnet"}
mapfile -t networks_list < <(echo "${build_networks}" | tr ',' '\n'| awk '{$1=$1; print}')
# Convert platform list to JSON array
networks_json=$(jq -n --argjson p "$(printf '%s\n' "${networks_list[@]}" | jq -R . | jq -s .)" '$p')
# Final merged matrix
matrix_final="[]"

for net in "${networks_list[@]}"; do
  source ../multinet_envs.sh ${net}
  # Add tari_network and tari_target_network for this network
  matrix_network=$(echo "${matrix_platforms}" | \
    jq --arg tn "${net}" --arg ttn "${TARI_TARGET_NETWORK}" 'map(. + {"tari_network": $tn, "tari_target_network": $ttn})')

  # Merge into final matrix
  matrix_final=$(echo "${matrix_final} ${matrix_network}" | jq -s 'add')
done

#matrix=$(echo "${matrix_platforms}" | jq -s -c '{"builds": .[]}')
matrix=$(echo "${matrix_final}" | jq -s -c '{"builds": .[]}')

echo "${matrix}"
echo "${matrix}" | jq .
