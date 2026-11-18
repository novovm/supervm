# B automatic existing recovery — A/B receipt

Started from the current C/D observer evidence at `7f9841a8`: C node
25544 / observer 5264 and D node 34148 / observer 24100. The fresh A→C
authenticated Status was 40 ms, with original chain height and state root.

B's original `novovm-B-55cafb07-network-20261118.service` automatically
replaced PID 3167799 with 3513819 after one targeted process exit, with
`NRestarts` 0→1, a new InvocationID, `START=existing`, the same database,
config, network and custody, restored UDP listeners and unchanged other
services. No manual start. The original process-to-process gap was
5009.551 ms, exit to observed local health 8399.785 ms. Independent A→B
authenticated Status passed in 69 ms at height 1 and the original root.

The first orchestration stopped without sending when the empty circuit
had not reported a drop in five seconds. A later snapshot confirms the
old circuit is closed and the reservation released; the failure cleanup
also issued cancellation, so this does not isolate the cause or exact
latency of remote drop detection. The continuation did not signal or
restart B again. It used the original owner to authenticate a different
circuit/session and send exactly one new object. A recorded sender DONE
and B recorded 9/9 forwarding packets, 9380/9044 bytes, zero failed
writes, on the same new circuit and route digest.

- Circuit: `2e2b74fce46b59cedf4e4dec25c72db2`
- Route digest: `b3d04da668d6d933145b41ddaf48671605d355cd8eacda2a276a1ef9d4a26fc7`
- Native Session: `2139389cd441096ebb9e53ebe190d6bc`
- Object: `9f5cfb4ffe1a8a3a0a16e289d04e0bea0389488fb15e075d979a68a939af260f`

The second-hop C forwarding and D's independently verified object custody
are pending their raw returns. This receipt is not double-relay end-to-end
acceptance. Do not restart nodes, repeat the recovery or resend the object.

Raw observations retained on A:

| Original file | SHA256 |
| --- | --- |
| First `A-after-B-automatic-restart.json` | `5ef67121f1a395d8142829f4b219ebd5ccb81ac861f61614d354621a7fbc2abd` |
| First `error.json` | `df33570a3f5b2b962a1f4e75098abbcb422f17d1d421e2d64df60ed43b6dcab6` |
| Continuation `A-late-circuit-closure.json` | `4337f66d7e436eca86a05f37e1dc216e697896430e51041b6ff0957885cd12e5` |
| Continuation `after-B-automatic-restart-after.json` | `55093daabfbdf0941079948f6604a20279f5c5d15fb3a5f2eae6e115bab5fb0d` |
| Continuation `B-after-recovery-forwarding.json` | `c6b199e352b00e68b9622e506c33440e7b8442ecd66b87bfc1d74c988ffc8cfa` |
