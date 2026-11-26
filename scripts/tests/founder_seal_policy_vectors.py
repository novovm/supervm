"""Offline specification vectors only; no real signatures or node integration."""
from dataclasses import dataclass, replace
import unittest


@dataclass(frozen=True)
class Policy:
    members: tuple
    transition_weight: int
    incoming_weight: int
    seal_weight: int
    founder_expiry: int


@dataclass(frozen=True)
class State:
    domain: str
    head: str
    epoch: int
    suite: str
    sealed: bool
    policy: Policy


@dataclass(frozen=True)
class Proof:
    # Each entry represents a fixture assertion of independent authentication
    # over the same complete action commitment, not actual signature bytes.
    action: tuple
    signers: tuple


def qualifies(proof, action, members, threshold):
    weights = dict(members)
    return (
        proof.action == action
        and len(set(proof.signers)) == len(proof.signers)
        and all(signer in weights for signer in proof.signers)
        and sum(weights[s] for s in proof.signers) >= threshold
    )


def authorization(state, role, action, height):
    if action == "spend_own_assets":
        return role == "ordinary_asset_owner"
    if action == "revive_founder_privilege":
        return False
    return (
        role == "founder_transition"
        and action == "approved_transition_operations"
        and not state.sealed
        and height < state.policy.founder_expiry
    )


def seal(state, action, proof, finality, exit_evidence, pending_transition):
    if state.sealed:
        raise ValueError("already sealed")
    expected = ("seal", state, action[2])
    if (action != expected or not finality or not exit_evidence or pending_transition
            or not qualifies(proof, action, state.policy.members, state.policy.seal_weight)):
        raise ValueError("seal gate not met")
    return replace(state, sealed=True)


def handoff(state, action, outgoing, incoming, finality, height, supported_suites):
    kind, anchor, successor, activation = action
    if (kind != "handoff" or anchor != state or not finality
            or height != activation or successor.epoch != state.epoch + 1
            or successor.domain != state.domain or successor.head != state.head
            or successor.sealed != state.sealed
            or successor.policy.founder_expiry != state.policy.founder_expiry
            or successor.suite not in supported_suites):
        raise ValueError("activation/context mismatch")
    if not qualifies(outgoing, action, state.policy.members, state.policy.transition_weight):
        raise ValueError("outgoing authorization missing")
    if not qualifies(incoming, action, successor.policy.members, successor.policy.incoming_weight):
        raise ValueError("incoming readiness missing")
    return successor


class FounderSealVectors(unittest.TestCase):
    def setUp(self):
        # Fictional fixture weights, heights, roles and signature suite.
        self.policy = Policy((("a", 2), ("b", 1), ("c", 1)), 3, 3, 4, 30)
        self.old = State("fixture-chain", "fixture-finalized-head", 7, "fixture-v1", True, self.policy)
        self.new = replace(self.old, epoch=8, policy=replace(
            self.policy, members=(("d", 2), ("e", 1), ("f", 1))))
        self.seal_base = replace(self.old, sealed=False)

    def transfer(self, *, new=None, old_signers=("a", "b"), new_signers=("d", "e"),
                 anchor=None, signed_action=None, height=50, finality=True):
        action = ("handoff", anchor or self.old, new or self.new, 50)
        return handoff(self.old, action, Proof(signed_action or action, old_signers),
                       Proof(action, new_signers), finality, height, {"fixture-v1", "fixture-v2"})

    def apply_seal(self, state=None, signers=("a", "b", "c"), finality=True,
                   exit_evidence=True, pending=False):
        state = state or self.seal_base
        action = ("seal", state, "fixture-exit-manifest")
        return seal(state, action, Proof(action, signers), finality, exit_evidence, pending)

    def test_valid_handoff(self):
        self.assertEqual(self.transfer(), self.new)

    def test_old_set_cannot_authorize_after_handoff(self):
        active = self.transfer()
        action = ("handoff", active, replace(active, epoch=9), 51)
        with self.assertRaisesRegex(ValueError, "outgoing"):
            handoff(active, action, Proof(action, ("a", "b")), Proof(action, ("d", "e")),
                    True, 51, {"fixture-v1"})

    def test_insufficient_outgoing_halts(self):
        with self.assertRaisesRegex(ValueError, "outgoing"):
            self.transfer(old_signers=("a",))
        self.assertEqual(self.transfer(), self.new)

    def test_insufficient_incoming_halts(self):
        with self.assertRaisesRegex(ValueError, "incoming"):
            self.transfer(new_signers=("d",))

    def test_duplicate_signer_does_not_add_weight(self):
        with self.assertRaisesRegex(ValueError, "outgoing"):
            self.transfer(old_signers=("a", "a"))

    def test_wrong_parent_and_early_activation(self):
        with self.assertRaisesRegex(ValueError, "context"):
            self.transfer(anchor=replace(self.old, head="wrong"))
        with self.assertRaisesRegex(ValueError, "context"):
            self.transfer(height=49)

    def test_transplant_from_other_action_rejected(self):
        with self.assertRaisesRegex(ValueError, "outgoing"):
            self.transfer(signed_action=("handoff", self.old, self.new, 49))

    def test_valid_seal_is_once_only(self):
        sealed = self.apply_seal()
        self.assertTrue(sealed.sealed)
        with self.assertRaisesRegex(ValueError, "already sealed"):
            self.apply_seal(sealed)

    def test_seal_requires_exit_evidence_authorization_and_finality(self):
        for options in ({"signers": ("a", "b")}, {"finality": False},
                        {"exit_evidence": False}, {"pending": True}):
            with self.subTest(options=options), self.assertRaisesRegex(ValueError, "seal gate"):
                self.apply_seal(**options)

    def test_old_key_expiry_and_seal_do_not_remove_ordinary_asset_rights(self):
        for state, height, expected in ((self.seal_base, 29, True), (self.seal_base, 30, False),
                                        (self.old, 29, False)):
            self.assertEqual(authorization(state, "founder_transition",
                                           "approved_transition_operations", height), expected)
            self.assertFalse(authorization(state, "founder_transition", "revive_founder_privilege", height))
            self.assertTrue(authorization(state, "ordinary_asset_owner", "spend_own_assets", height))

    def test_legal_successor_crypto_migration_preserves_seal(self):
        successor = replace(self.new, suite="fixture-v2")
        self.assertEqual(self.transfer(new=successor), successor)
        self.assertTrue(successor.sealed)

    def test_migration_cannot_unseal_use_unknown_suite_or_extend_expiry(self):
        for successor in (replace(self.new, sealed=False), replace(self.new, suite="unknown"),
                          replace(self.new, policy=replace(self.new.policy, founder_expiry=99))):
            with self.subTest(successor=successor), self.assertRaisesRegex(ValueError, "context"):
                self.transfer(new=successor)

    def test_authorization_does_not_replace_finality(self):
        with self.assertRaisesRegex(ValueError, "context"):
            self.transfer(finality=False)


if __name__ == "__main__":
    unittest.main(verbosity=2)
