#!/bin/bash

CHECK_ONLY=0
# Check if the first command-line argument is '-c'
if [[ $1 == "-c" ]]; then
  CHECK_ONLY=1
fi

# Declare associative arrays
declare -A package_group_map
declare -A group_user_map

# Populate group_user_map
group_user_map["ignore"]="CjS77 stringhandler SWvheerden"
group_user_map["leads"]="CjS77 stringhandler SWvheerden"
group_user_map["dan"]="CjS77 stringhandler sdbondi"

# Minotari crates and libraries

package_group_map["minotari_app_grpc"]="leads"
package_group_map["minotari_app_utilities"]="leads"
package_group_map["minotari_chat_ffi"]="leads"
package_group_map["minotari_console_wallet"]="leads"
package_group_map["minotari_merge_mining_proxy"]="leads"
package_group_map["minotari_miner"]="leads"
package_group_map["minotari_mining_helper_ffi"]="leads"
package_group_map["minotari_wallet"]="leads"
package_group_map["minotari_wallet_ffi"]="leads"
package_group_map["minotari_node"]="leads"
package_group_map["tari_crypto"]="leads"
package_group_map["tari_common"]="leads"
package_group_map["tari_utilities"]="leads"
package_group_map["tari_bulletproofs_plus"]="leads"
package_group_map["tari_comms_dht"]="leads"
package_group_map["tari_core"]="leads"
package_group_map["tari_common_types"]="leads"
package_group_map["tari_comms"]="leads"
package_group_map["tari_key_manager"]="leads"
package_group_map["tari_p2p"]="leads"
package_group_map["tari_protobuf_build"]="leads"
package_group_map["tari_script"]="leads"
package_group_map["tari_features"]="ignore"
package_group_map["tari_comms_rpc_macros"]="leads"
package_group_map["tari_contacts"]="leads"
package_group_map["tari_service_framework"]="leads"

# Tari/DAN crates and libraries
package_group_map["tari_template_lib"]="dan"
package_group_map["tari_dan_app_utilities"]="dan"
package_group_map["tari_dan_common_types"]="dan"
package_group_map["tari_dan_engine"]="dan"
package_group_map["tari_dan_p2p"]="dan"
package_group_map["tari_dan_storage"]="dan"
package_group_map["tari_dan_storage_lmdb"]="dan"
package_group_map["tari_dan_storage_sqlite"]="dan"
package_group_map["tari_dan_wallet_cli"]="dan"
package_group_map["tari_dan_wallet_daemon"]="dan"
package_group_map["tari_engine_types"]="dan"
package_group_map["tari_epoch_manager"]="dan"
package_group_map["tari_state_store_sqlite"]="dan"
package_group_map["tari_template_abi"]="dan"
package_group_map["tari_template_builtin"]="dan"
package_group_map["tari_template_macros"]="dan"
package_group_map["tari_template_test_tooling"]="dan"
package_group_map["tari_transaction"]="dan"
package_group_map["tari_transaction_manifest"]="dan"
package_group_map["tari_indexer"]="dan"
package_group_map["tari_indexer_client"]="dan"
package_group_map["tari_indexer_lib"]="dan"

# Deprecated, unused, or unclassified packages.
package_group_map["tari_signaling_server"]="ignore"
package_group_map["tari_bor"]="ignore"
package_group_map["tari_comms_logging"]="ignore"
package_group_map["tari_comms_rpc_state_sync"]="ignore"
package_group_map["tari_consensus"]="ignore"
package_group_map["tari_wallet_ffi"]="ignore"
package_group_map["tari_storage"]="ignore"
package_group_map["tari_wallet"]="ignore"
package_group_map["tari_comms_middleware"]="ignore"
package_group_map["tari_infra_derive"]="ignore"
package_group_map["tari-curve25519-dalek"]="ignore"
package_group_map["tari_shutdown"]="ignore"
package_group_map["tari_mmr"]="ignore"
package_group_map["tari_base_node"]="ignore"
package_group_map["tari_base_node_client"]="ignore"
package_group_map["tari_broadcast_channel"]="ignore"
package_group_map["tari_bulletproofs"]="ignore"
package_group_map["tari_validator_node"]="ignore"
package_group_map["tari_validator_node_cli"]="ignore"
package_group_map["tari_validator_node_client"]="ignore"
package_group_map["tari_validator_node_rpc"]="ignore"
package_group_map["tari_wallet_daemon_client"]="ignore"
package_group_map["tari_transactions"]="ignore"
package_group_map["tari_mining"]="ignore"
package_group_map["tari_mmr_integration_tests"]="ignore"
package_group_map["tari_pubsub"]="ignore"
package_group_map["tari_test_utils"]="ignore"
package_group_map["tari_libtor"]="ignore"
package_group_map["tari_metrics"]="ignore"
package_group_map["tari_scaffolder"]="ignore"

##########################  Owner management functions  ##########################
remove_owner() {
  echo "Removing $1 as owner of $package"
  cargo owner -q --remove $1 $package
}

verify_owner() {
  # No-op
  :
}

add_owner() {
  echo "Adding $1 to $package"
  cargo owner -q --add $1 $package
}

##################################  Main script  ##################################

# Iterate over packages
for package in "${!package_group_map[@]}"; do
  echo ""
  echo "Processing $package..."
  # Get the expected owners
  group=${package_group_map[$package]}
  # If group is 'ignore', skip this iteration
  if [[ $group == "ignore" ]]; then
    echo "Ignoring $package"
    continue
  fi
  expected_owners=(${group_user_map[$group]})

  # Get the current owners
  current_owners=($(cargo owner -q --list $package | awk '{print $1}'))

  # Convert the arrays to space-separated strings for comparison
  current_owners_str=" ${current_owners[*]} "
  expected_owners_str=" ${expected_owners[*]} "

  echo "Current owners vs: $current_owners_str"
  echo "Expected owners  : $expected_owners_str"

  if [[ $CHECK_ONLY == 1 ]]; then
    continue
  fi

  # Iterate over the current owners
  for user in "${current_owners[@]}"; do
    if [[ $expected_owners_str == *" $user "* ]]; then
      # User is in both current and expected owners
      verify_owner $user
    else
      # User is in current owners but not in expected owners
      remove_owner $user
    fi
  done

  # Iterate over the expected owners
  for user in "${expected_owners[@]}"; do
    if [[ $current_owners_str != *" $user "* ]]; then
      # User is in expected owners but not in current owners
      add_owner $user
    fi
  done
  echo "... Done processing $package"
  echo ""
  # To avoid 429 Too Many Requests, sleep for 5 seconds between packages
  sleep 5
done
