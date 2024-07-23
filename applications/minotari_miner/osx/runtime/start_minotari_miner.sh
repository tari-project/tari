#!/bin/bash
#
#  Copyright 2024. The Tari Project
#
#  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
#  following conditions are met:
#
#  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
#  disclaimer.
#
#  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
#  following disclaimer in the documentation and/or other materials provided with the distribution.
#
#  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
#  products derived from this software without specific prior written permission.
#
#  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
#  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
#  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
#  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
#  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
#  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
#  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
#

#
echo
echo "Starting Miner"
echo

# Initialize
if [ -z "${use_parent_paths}" ]
then
    base_path="$( cd "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"
    config_path="${base_path}/config"
    exe_path="${base_path}/runtime"
fi

tor_running=$(lsof -nP -iTCP:9050)
if [ -z "${tor_running}" ]
then
    echo "Starting Tor"
    open -a Terminal.app "${exe_path}/start_tor.sh"
    ping -c 15 localhost > /dev/null
fi

if [ ! -f "${config_path}/log4rs_miner.yml" ]
then
    echo Creating new "${config_path}/log4rs_miner.yml";
    init_flag="--init"
else
    echo Using existing "${config_path}/log4rs_miner.yml";
    init_flag=""
fi
echo

# Run
echo Spawning Console Wallet into new terminal..
echo "${exe_path}/minotari_miner" ${init_flag} --config="${config_path}/config.toml" --log_config="${config_path}/log4rs_miner.yml" --base-path=${base_path} > $exe_path/tari_miner_command.sh
chmod +x $exe_path/minotari_miner_command.sh

open -a terminal $exe_path/minotari_miner_command.sh
echo
