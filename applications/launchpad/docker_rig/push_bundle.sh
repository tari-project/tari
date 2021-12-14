# Copyright 2021. The Tari Project
#
# Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
# following conditions are met:
#
# 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
# disclaimer.
#
# 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
# following disclaimer in the documentation and/or other materials provided with the distribution.
#
# 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
# products derived from this software without specific prior written permission.
#
# THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
# INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
# DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
# SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
# SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
# WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
# USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
#

 #!/usr/local/bin/bash
export DATA_FOLDER="/tmp/push_bundle"
export TARI_NETWORK=weatherwax

check_data_folder() {
  if [[ ! -d "$DATA_FOLDER" ]]; then
    echo "Creating data folder $DATA_FOLDER.."
    mkdir -p "$DATA_FOLDER"
    mkdir -p "$DATA_FOLDER/tor"
    mkdir -p "$DATA_FOLDER/xmrig"
    mkdir -p "$DATA_FOLDER/monerod"
    mkdir -p "$DATA_FOLDER/mm_proxy"
    cp torrc "$DATA_FOLDER/tor"
    CREATE_CONFIG=1
    CREATE_ID=1
    echo "Done."
  else
    echo "Using existing data folder $DATA_FOLDER"
  fi
}

check_data_folder

declare -A versions

versions["tari_base_node"]="$(docker compose run --rm base_node --version | awk '/tari_common /{print $NF}')"
versions["wallet"]="$(docker compose run --rm wallet --version | awk '/tari_common /{print $NF}')"
versions["sha3_miner"]="$(docker compose run --rm sha3_miner --version | awk '/tari_common /{print $NF}')"
versions["mm_proxy"]="$(docker compose run --rm mm_proxy --version | awk '/tari_common /{print $NF}')"
versions["xmrig"]="$(docker compose run --rm xmrig --version | awk '/XMRig /{print $NF}')"
versions["tor"]="$(docker compose run --rm tor tor --version | awk '/Tor version /{print $NF}')"
versions["monerod"]="$(docker compose run --rm monerod --version | sed  's/Monero .*(\(.*\)-.*)/\1/')"

echo "${versions[@]}"

for i in tari_base_node wallet sha3_miner mm_proxy_ver xmrig_ver tor_ver monerod; do
  docker image tag quay.io/tarilabs/$i:latest quay.io/tarilabs/$i:${versions[$i]}
  docker push quay.io/tarilabs/$i:${versions[$i]}
  docker push quay.io/tarilabs/$i:latest
done