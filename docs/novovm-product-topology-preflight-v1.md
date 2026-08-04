# NOVOVM Product Mainline Topology Preflight v1

`novovm-product-topology` validates a set of main-node Product Overlay configs
before they are copied to separate machines. It is an offline configuration
gate, not a network test and not public-topology evidence.

Run:

```bash
novovm-product-topology topology-plan.json
```

Example plan:

```json
{
  "scope": "novovm_product_mainline_topology_plan_v1",
  "chain_id": 1,
  "require_identity_files": false,
  "nodes": [
    {
      "name": "node-a",
      "peer_id": "novovm-ed25519:<node-a-public-key>",
      "config_path": "node-a.json"
    },
    {
      "name": "node-b",
      "peer_id": "novovm-ed25519:<node-b-public-key>",
      "config_path": "node-b.json"
    },
    {
      "name": "node-c",
      "peer_id": "novovm-ed25519:<node-c-public-key>",
      "config_path": "node-c.json"
    },
    {
      "name": "node-d",
      "peer_id": "novovm-ed25519:<node-d-public-key>",
      "config_path": "node-d.json"
    }
  ]
}
```

Config paths are resolved from the plan file directory. Paths inside each node
config are resolved from that config's directory. No common drive letter,
workspace root, or process working directory is required.

The preflight enforces:

- 2 to 64 unique nodes on one positive chain ID;
- one explicit `duplex` multi-peer config per node;
- unique peer and metric IDs inside each node config;
- the exact symmetric full mesh: every node lists every other node once;
- optional local identity-file verification when
  `require_identity_files=true`.

The JSON result always keeps:

```text
external_network_executed=false
real_public_topology_proven=false
real_cross_nat_proven=false
```

An accepted preflight means only that the topology fields listed above agree. It does not compare
release fingerprints, relay wire/report versions, resource-limit profiles, package checksums, or
signed-directory capacity against live daemon reports. It also does not mean that WSS, VPS, NAT,
cellular, CGNAT, VPN/TUN, cross-machine AOEM execution, or long-run recovery has been executed.
Deploy the identical clean-worktree package checksum to every node; mixed-version rolling operation
is not claimed by this preflight.
