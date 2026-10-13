//! Versioned bindings/guards over existing resident programs, not an interpreter.
use super::*;

/// `program` indexes the supplied owner-bound resource table. A false Boolean
/// guard skips the program entirely and yields `otherwise`; it does not execute
/// a faulting branch and multiply its result by zero. Programs remain pure;
/// business failure must be a checked Value, not a domain/transport failure.
#[derive(Clone, Debug)]
pub struct AoemIntegerPlanNodeV1 {
    pub program: usize,
    pub guard: AoemIntegerInputV1,
    pub inputs: Vec<AoemIntegerInputV1>,
    pub otherwise: Vec<AoemIntegerInputV1>,
}

const MAX_NODES: usize = 65_536;
const MAX_VALUES: usize = 131_072;
const MAX_BYTES: usize = 32 * 1024 * 1024;

fn binding(body: &mut Vec<u8>, value: AoemIntegerInputV1, arities: &[usize]) -> Result<()> {
    match value {
        AoemIntegerInputV1::Literal(v) => {
            let compact = v.0[4..].iter().all(|n| *n == 0);
            body.extend((if compact { 2u32 } else { 0 }).to_le_bytes());
            for &limb in &v.0[..if compact { 4 } else { 32 }] {
                body.extend(limb.to_le_bytes());
            }
        }
        AoemIntegerInputV1::Output { row, column } => {
            if arities
                .get(row as usize)
                .is_none_or(|n| column as usize >= *n)
            {
                bail!("composition forward/absent output reference");
            }
            for n in [1, row, column] {
                body.extend(n.to_le_bytes());
            }
        }
    }
    if body.len() > MAX_BYTES {
        bail!("composition byte budget");
    }
    Ok(())
}

fn decode(bytes: &[u8], id: u64, sequence: u64, count: usize) -> Result<Vec<AoemInteger1024V1>> {
    if bytes.len() != 32 + count * 128
        || &bytes[..8] != b"AOIR0002"
        || bytes[8..16] != id.to_le_bytes()
        || bytes[16..24] != sequence.to_le_bytes()
        || bytes[24..28] != (count as u32).to_le_bytes()
        || bytes[28..32] != 32u32.to_le_bytes()
    {
        bail!("composition binary result identity/shape/length");
    }
    Ok(bytes[32..]
        .chunks_exact(128)
        .map(|v| {
            let mut limbs = [0; 32];
            for (slot, bytes) in limbs.iter_mut().zip(v.chunks_exact(4)) {
                *slot = u32::from_le_bytes(bytes.try_into().unwrap());
            }
            AoemInteger1024V1(limbs)
        })
        .collect())
}

impl<T: ResourceTransport> ResourceOwner<T> {
    pub(super) fn run_composition(
        &mut self,
        resources: &[&AoemIntegerResourceV7],
        nodes: &[AoemIntegerPlanNodeV1],
        outputs: &[AoemIntegerInputV1],
    ) -> Result<Vec<AoemInteger1024V1>> {
        self.healthy()?;
        if !(1..=8).contains(&resources.len())
            || !(1..=MAX_NODES).contains(&nodes.len())
            || !(1..=MAX_NODES).contains(&outputs.len())
        {
            bail!("composition count budget");
        }
        let mut shapes = Vec::with_capacity(resources.len());
        let mut ids = Vec::with_capacity(resources.len());
        let mut body = Vec::new();
        body.extend((resources.len() as u32).to_le_bytes());
        body.extend((outputs.len() as u32).to_le_bytes());
        for resource in resources {
            let shape = self.shape(resource)?;
            if shape.inputs == 0 || ids.contains(&resource.id) {
                bail!("composition resource table");
            }
            shapes.push(shape);
            ids.push(resource.id);
            body.extend(resource.id.to_le_bytes());
        }
        let mut arities = Vec::with_capacity(nodes.len());
        let mut values = 0;
        for node in nodes {
            let shape = shapes
                .get(node.program)
                .context("composition program index")?;
            if node.inputs.len() != shape.inputs || node.otherwise.len() != shape.outputs {
                bail!("composition node arity");
            }
            values += shape.outputs;
            if values > MAX_VALUES {
                bail!("composition intermediate budget");
            }
            body.extend((node.program as u32).to_le_bytes());
            if let AoemIntegerInputV1::Literal(value) = node.guard {
                if value.try_to_u128()? > 1 {
                    bail!("composition guard is not Boolean");
                }
            }
            binding(&mut body, node.guard, &arities)?;
            for &input in node.inputs.iter().chain(&node.otherwise) {
                binding(&mut body, input, &arities)?;
            }
            arities.push(shape.outputs);
        }
        for &output in outputs {
            binding(&mut body, output, &arities)?;
        }
        let sequence = REQUEST_SEQUENCE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| anyhow::anyhow!("integer request sequence exhausted"))?
            + 1;
        let id = resources[0].id;
        let mut payload = resource_header(2, id, nodes.len(), 0, body.len());
        payload[8..12].copy_from_slice(&10u32.to_le_bytes());
        payload.extend(sequence.to_le_bytes());
        payload.extend(body);
        self.poisoned = true;
        let bytes = self
            .transport
            .exchange_binary(&self.prefix, &payload, id, sequence)?;
        let values = decode(&bytes, id, sequence, outputs.len())?;
        self.poisoned = false;
        Ok(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Transport {
        calls: usize,
        malformed: bool,
    }
    impl ResourceTransport for Transport {
        fn exchange(&mut self, _: &str, _: &[u8]) -> Result<serde_json::Value> {
            panic!("composition must not use JSON");
        }
        fn exchange_binary(
            &mut self,
            _: &str,
            payload: &[u8],
            id: u64,
            sequence: u64,
        ) -> Result<Vec<u8>> {
            self.calls += 1;
            assert_eq!(&payload[8..12], &10u32.to_le_bytes());
            let mut result = b"AOIR0002".to_vec();
            result.extend(id.to_le_bytes());
            result.extend(sequence.to_le_bytes());
            result.extend(1u32.to_le_bytes());
            result.extend(32u32.to_le_bytes());
            for n in [42u32].into_iter().chain([0; 31]) {
                result.extend(n.to_le_bytes());
            }
            if self.malformed {
                result.push(0);
            }
            Ok(result)
        }
    }
    #[test]
    fn composition_validates_before_submit_and_latches_bad_binary() {
        let mut owner = ResourceOwner {
            transport: Transport {
                calls: 0,
                malformed: false,
            },
            owner: 17,
            prefix: "test/composition".into(),
            resources: BTreeMap::new(),
            last_resource_id: 2,
            poisoned: false,
        };
        let shape = Shape {
            rows: 2,
            inputs: 1,
            outputs: 1,
            domains: 0,
            refutations: 0,
            postconditions: 1,
        };
        owner.resources.insert(1, shape);
        owner.resources.insert(2, shape);
        let a = AoemIntegerResourceV7 { owner: 17, id: 1 };
        let b = AoemIntegerResourceV7 { owner: 17, id: 2 };
        let literal = |n| AoemIntegerInputV1::Literal(AoemInteger1024V1::from_u128(n));
        let edge = |row| AoemIntegerInputV1::Output { row, column: 0 };
        let mut nodes = vec![
            AoemIntegerPlanNodeV1 {
                program: 0,
                guard: literal(1),
                inputs: vec![literal(41)],
                otherwise: vec![literal(0)],
            },
            AoemIntegerPlanNodeV1 {
                program: 1,
                guard: literal(1),
                inputs: vec![edge(0)],
                otherwise: vec![literal(0)],
            },
        ];
        let resources = [&a, &b];
        assert_eq!(
            owner
                .run_composition(&resources, &nodes, &[edge(1)])
                .unwrap()[0]
                .try_to_u128()
                .unwrap(),
            42
        );
        nodes[0].inputs[0] = edge(1);
        assert!(owner
            .run_composition(&resources, &nodes, &[edge(1)])
            .is_err());
        nodes[0].inputs[0] = literal(41);
        nodes[0].guard = literal(2);
        assert!(owner
            .run_composition(&resources, &nodes, &[edge(1)])
            .is_err());
        nodes[0].guard = literal(1);
        let foreign = AoemIntegerResourceV7 { owner: 18, id: 1 };
        assert!(owner
            .run_composition(&[&foreign, &b], &nodes, &[edge(1)])
            .is_err());
        assert_eq!(owner.transport.calls, 1);
        owner.transport.malformed = true;
        assert!(owner
            .run_composition(&resources, &nodes, &[edge(1)])
            .is_err());
        assert!(owner
            .run_composition(&resources, &nodes, &[edge(1)])
            .is_err());
        assert_eq!(owner.transport.calls, 2);
    }
}
