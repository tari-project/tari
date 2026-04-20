# Fixtures

## How to update the eviction proof fixture

To get a new eviction proof fixture, run the `single_shard_node_goes_down_and_gets_evicted` test in the consensus tests
on the ootle repo
(uncomment the lines the write the fixture to disk, and comment them out again after the fixture is generated).

## How to update the commit proof fixture

Easiest way is to take the `commit_proof` part from the eviction proof
