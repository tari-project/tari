# Copyright 2022 The Tari Project
# SPDX-License-Identifier: BSD-3-Clause

  # This file covers all of the GRPC functions for the base node
@base_node_grpc
Feature: Base Node GRPC

  Scenario: Base node lists blocks
    Given I have 1 seed nodes
    And I have a base node N1 connected to all seed nodes
    When I mine 5 blocks on N1
    Then node N1 lists blocks for heights 1 to 5

  Scenario: Base node lists headers
    Given I have 1 seed nodes
    And I have a base node BN1 connected to all seed nodes
    When I mine 5 blocks on BN1
    Then node BN1 lists headers 1 to 5 with correct heights

  @current
  Scenario: Base node GRPC - get header by height
    Given I have 1 seed nodes
    And I have a base node BN1 connected to all seed nodes
    When I mine 5 blocks on BN1
    When I get header by height 1 on BN1
    Then header is returned with height 1

 ####
#  // Lists headers in the current best chain
#  [x] rpc ListHeaders(ListHeadersRequest) returns (stream BlockHeader);
#  // Get header by hash
#  [] rpc GetHeaderByHash(GetHeaderByHashRequest) returns (BlockHeaderResponse);
#  // Get header by height
#  [] rpc GetHeaderByHeight(GetHeaderByHeightRequest) returns (BlockHeaderResponse);
#  // Returns blocks in the current best chain. Currently only supports querying by height
#  rpc GetBlocks(GetBlocksRequest) returns (stream HistoricalBlock);
#  // Returns the block timing for the chain heights
#  rpc GetBlockTiming(HeightRequest) returns (BlockTimingResponse);
#  // Returns the network Constants
#  rpc GetConstants(Empty) returns (ConsensusConstants);
#  // Returns Block Sizes
#  rpc GetBlockSize (BlockGroupRequest) returns (BlockGroupResponse);
#  // Returns Block Fees
#  rpc GetBlockFees (BlockGroupRequest) returns (BlockGroupResponse);
#  // Get Version
#  rpc GetVersion(Empty) returns (StringValue);
#  // Check for new updates
#  rpc CheckForUpdates(Empty) returns (SoftwareUpdate);
#  // Get coins in circulation
#  rpc GetTokensInCirculation(GetBlocksRequest) returns (stream ValueAtHeightResponse);
#  // Get network difficulties
#  rpc GetNetworkDifficulty(HeightRequest) returns (stream NetworkDifficultyResponse);
#  // Get the block template
#  rpc GetNewBlockTemplate(NewBlockTemplateRequest) returns (NewBlockTemplateResponse);
#  // Construct a new block from a provided template
#  rpc GetNewBlock(NewBlockTemplate) returns (GetNewBlockResult);
#  // Construct a new block and header blob from a provided template
#  rpc GetNewBlockBlob(NewBlockTemplate) returns (GetNewBlockBlobResult);
#  // Submit a new block for propagation
#  rpc SubmitBlock(Block) returns (SubmitBlockResponse);
#  // Submit a new mined block blob for propagation
#  rpc SubmitBlockBlob(BlockBlobRequest) returns (SubmitBlockResponse);
#  // Submit a transaction for propagation
#  rpc SubmitTransaction(SubmitTransactionRequest) returns (SubmitTransactionResponse);
#  // Get the base node sync information
#  rpc GetSyncInfo(Empty) returns (SyncInfoResponse);
#  // Get the base node sync information
#  rpc GetSyncProgress(Empty) returns (SyncProgressResponse);
#  // Get the base node tip information
#  rpc GetTipInfo(Empty) returns (TipInfoResponse);
#  // Search for blocks containing the specified kernels
#  rpc SearchKernels(SearchKernelsRequest) returns (stream HistoricalBlock);
#  // Search for blocks containing the specified commitments
#  rpc SearchUtxos(SearchUtxosRequest) returns (stream HistoricalBlock);
#  // Fetch any utxos that exist in the main chain
#  rpc FetchMatchingUtxos(FetchMatchingUtxosRequest) returns (stream FetchMatchingUtxosResponse);
#  // get all peers from the base node
#  rpc GetPeers(GetPeersRequest) returns (stream GetPeersResponse);
#  rpc GetMempoolTransactions(GetMempoolTransactionsRequest) returns (stream GetMempoolTransactionsResponse);
#  rpc TransactionState(TransactionStateRequest) returns (TransactionStateResponse);
#  // This returns the node's network identity
#  rpc Identify (Empty) returns (NodeIdentity);
#  // Get Base Node network connectivity status
#  rpc GetNetworkStatus(Empty) returns (NetworkStatusResponse);
#  // List currently connected peers
#  rpc ListConnectedPeers(Empty) returns (ListConnectedPeersResponse);
#  // Get mempool stats
#  rpc GetMempoolStats(Empty) returns (MempoolStatsResponse);
#
#  rpc GetTokens(GetTokensRequest) returns (stream GetTokensResponse);
#  rpc ListAssetRegistrations(ListAssetRegistrationsRequest) returns (stream ListAssetRegistrationsResponse);
#  rpc GetAssetMetadata(GetAssetMetadataRequest) returns (GetAssetMetadataResponse);
#
#  // Get all constitutions where the public key is in the committee
#  rpc GetConstitutions(GetConstitutionsRequest) returns (stream GetConstitutionsResponse);
#  // Get the current contract outputs matching the given contract id and type
#  rpc GetCurrentContractOutputs(GetCurrentContractOutputsRequest) returns (GetCurrentContractOutputsResponse);
#  }
