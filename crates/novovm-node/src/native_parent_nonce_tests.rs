mod native_parent_nonce_tests {
    use super::*;
    use novovm_protocol::native_parent_nonce::{native_state_wire_root_v3, parent_nonce_v3};

    fn snapshot(state: &NovNativeExecutionModuleStateV1) -> Vec<u8> {
        canonical_json_value_wire_v1(&native_committed_module_state_projection_v3(state)).unwrap()
    }
    #[test]
    fn parent_nonce_reads_real_host_projection_and_preserves_root() {
        let mut state = NovNativeExecutionModuleStateV1::default();
        let key = [7; 32];
        let identity = novovm_protocol::native_nonce::signer_nonce_identity_v2(&key);
        let index = native_auth_nonce_identity_key_v1(1, &identity);
        state.native_auth_next_nonces.insert(index, 19);
        let wire = snapshot(&state);
        let root = native_state_wire_root_v3(&wire);
        assert_eq!(
            to_hex(&root),
            native_semantic_ledger_state_digest_v1(&state)
        );
        assert_eq!(
            root,
            sha256_bytes_v1(&[
                b"novovm-native-aoem-semantic-ledger-state-digest-v3\0",
                &wire
            ])
        );
        assert_eq!(parent_nonce_v3(&wire, &root, 1, &key), Ok(19));
        assert_eq!(parent_nonce_v3(&wire, &root, 2, &key), Ok(0));
        assert_eq!(parent_nonce_v3(&wire, &root, 1, &[8; 32]), Ok(0));
        assert!(parent_nonce_v3(&wire, &[0; 32], 1, &key).is_err());
        let mut changed_state = state.clone();
        changed_state
            .account_asset_balances
            .entry("changed-account".into())
            .or_default()
            .insert("NOV".into(), 999);
        assert!(parent_nonce_v3(&snapshot(&changed_state), &root, 1, &key).is_err());
        let mut tampered = wire.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(parent_nonce_v3(&tampered, &root, 1, &key).is_err());
        super::native_state_root_v3_binds_semantic_head_and_protocol_config();
    }

    #[test]
    fn parent_nonce_missing_tables_and_invalid_nonce_never_become_genesis() {
        let state = NovNativeExecutionModuleStateV1::default();
        let key = [7; 32];
        let identity = novovm_protocol::native_nonce::signer_nonce_identity_v2(&key);
        let index = native_auth_nonce_identity_key_v1(1, &identity);
        for case in 0..8 {
            let mut projection = native_committed_module_state_projection_v3(&state);
            let execution = &mut projection["module_state_shards"]["native_execution"];
            match case {
                0 => {
                    execution
                        .as_object_mut()
                        .unwrap()
                        .remove("native_auth_next_nonces");
                }
                1 => execution["native_auth_next_nonces"] = serde_json::Value::Null,
                2 => execution["native_auth_nonce_identity_scheme"] = serde_json::json!("old"),
                3 => execution["native_auth_next_nonces"][&index] = serde_json::json!("19"),
                4 => execution["native_auth_next_nonces"][&index] = serde_json::json!(-1),
                5 => execution["native_auth_next_nonces"][&index] = serde_json::json!(1.5),
                6 => projection["schema"] = serde_json::json!("wrong"),
                _ => {
                    projection
                        .as_object_mut()
                        .unwrap()
                        .remove("module_state_shards");
                }
            }
            let bytes = canonical_json_value_wire_v1(&projection).unwrap();
            assert!(
                parent_nonce_v3(&bytes, &native_state_wire_root_v3(&bytes), 1, &key).is_err(),
                "case {case}"
            );
        }
    }

    #[test]
    fn parent_nonce_rejects_truncated_trailing_duplicate_and_unbounded_wire() {
        for bytes in [
            vec![],
            vec![6, 255, 255, 255, 255, 255, 255, 255, 255],
            // Object containing the same key twice.
            [
                vec![6],
                2u64.to_be_bytes().to_vec(),
                1u64.to_be_bytes().to_vec(),
                vec![b'a', 0],
                1u64.to_be_bytes().to_vec(),
                vec![b'a', 0],
            ]
            .concat(),
            vec![7],
        ] {
            assert!(
                parent_nonce_v3(&bytes, &native_state_wire_root_v3(&bytes), 1, &[7; 32]).is_err()
            );
        }
        let mut bytes = snapshot(&NovNativeExecutionModuleStateV1::default());
        bytes.push(0);
        assert!(parent_nonce_v3(&bytes, &native_state_wire_root_v3(&bytes), 1, &[7; 32]).is_err());
        let deep = [[5, 0, 0, 0, 0, 0, 0, 0, 1].repeat(66), vec![0]].concat();
        assert!(parent_nonce_v3(&deep, &native_state_wire_root_v3(&deep), 1, &[7; 32]).is_err());
    }
}
