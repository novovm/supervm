use novovm_network::mailbox_replicas::*;
fn node(n: u8) -> ReplicaCandidateV1 {
    ReplicaCandidateV1 {
        node: [n; 32],
        admitted: true,
        durability: MailboxDurabilityV1::Durable,
        retain_until: 2000,
        available_bytes: 65536,
        domains: Some(FailureDomainsV1 {
            operator: [n; 32],
            device: [n; 32],
            network: [n; 32],
            power: [n; 32],
        }),
    }
}
#[test]
fn correlated_unknown_and_phone_copies_do_not_satisfy_durable_redundancy() {
    let a = node(1);
    let mut b = node(2);
    b.domains = a.domains;
    let mut c = node(3);
    c.durability = MailboxDurabilityV1::Opportunistic;
    let mut unknown = node(4);
    unknown.domains = None;
    let mut candidates = vec![a, b, c, unknown];
    let plan = select_mailbox_replicas_v1(&candidates, 10, 1500, 1000).unwrap();
    assert_eq!(plan.durable, vec![[1; 32]]);
    assert_eq!(plan.durable_shortfall, 1);
    assert_eq!(plan.opportunistic, Some([3; 32]));
    candidates.push(node(5));
    let plan = select_mailbox_replicas_v1(&candidates, 10, 1500, 1000).unwrap();
    assert_eq!(plan.durable, vec![[1; 32], [5; 32]]);
    assert_eq!(plan.durable_shortfall, 0);
    candidates[4].admitted = false;
    assert_eq!(
        select_mailbox_replicas_v1(&candidates, 10, 1500, 1000)
            .unwrap()
            .durable_shortfall,
        1
    );
    assert!(select_mailbox_replicas_v1(&candidates, 10, 1500, 1500).is_err());
    candidates[0].retain_until = 1499;
    candidates[1].available_bytes = 9;
    assert_eq!(
        select_mailbox_replicas_v1(&candidates, 10, 1500, 1000)
            .unwrap()
            .durable_shortfall,
        2
    );
}
