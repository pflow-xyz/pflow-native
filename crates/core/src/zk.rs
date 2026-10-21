//! Zero-knowledge proofs of firings: Groth16 over BN254 with Poseidon state
//! commitments, through pflow-rs's `pflow-zk` (incidence matrix, witness
//! types) and `pflow-zk-arkworks` (the transition circuit and the Poseidon
//! gadget). Behind the cargo feature `zk`: without it every entry point
//! refuses, and the commands that call them are greyed out.
//!
//! The circuit is the open document's incidence, compiled through the
//! metamodel (colors unfolded: one circuit place per place and color, in
//! display order). Its public inputs are (pre-state root, post-state
//! root, transition index); a proof says a committed marking steps to
//! another by that transition's column of the incidence matrix.
//!
//! Setup is run here, not through `ArkworksProver::setup` (which seeds its
//! RNG with a constant, so anyone could rebuild its toxic waste): the RNG is
//! seeded from the operating system and the trapdoor is dropped. Keys are
//! cached per circuit digest in memory and under
//! `<data_dir>/zk/<circuit digest>/`.

use serde_json::{json, Value};

use crate::artifact::TraceStep;
use crate::doc::Document;
use crate::provenance::{self, canonical};
use crate::sim::Marking;
use crate::stochastic::Net;
use crate::{Error, Result};

pub const SYSTEM: &str = "groth16-bn254";
pub const DEFAULT_TRACE_PROOFS: usize = 64;
pub const MAX_TRACE_PROOFS: usize = 256;

/// The open net as a transition circuit, before any key exists.
pub struct Circuit {
    pub net: Net,
    /// Transition ids, in model order (the circuit's transition index).
    pub transitions: Vec<String>,
    /// Per transition: (circuit place index, weight) of consuming arcs.
    pub inputs: Vec<Vec<(usize, i64)>>,
    /// Per transition: (circuit place index, weight) of producing arcs.
    pub outputs: Vec<Vec<(usize, i64)>>,
    /// sha256 over the canonical description of the circuit: what keys are
    /// cached under and what a proof names.
    pub digest: String,
    /// What the proof does not cover on this net.
    pub caveats: Vec<String>,
}

impl Circuit {
    pub fn new(doc: &Document) -> Result<Self> {
        let net = Net::new(doc)?;
        let idx = |p: &str| net.places.iter().position(|q| q == p);
        let m = &net.model;
        let transitions: Vec<String> = m.transitions.iter().map(|t| t.id.clone()).collect();
        let arcs = |list: Vec<pflow_metamodel::ArcRef>| -> Vec<(usize, i64)> {
            let mut v: Vec<(usize, i64)> = list
                .into_iter()
                .filter_map(|a| idx(&a.place).map(|i| (i, a.weight)))
                .collect();
            v.sort();
            v
        };
        let inputs: Vec<_> = transitions.iter().map(|t| arcs(m.inputs(t))).collect();
        let outputs: Vec<_> = transitions.iter().map(|t| arcs(m.outputs(t))).collect();
        if transitions.is_empty() || net.places.is_empty() {
            return Err(Error::Invalid(
                "the net needs at least one place and one transition to prove a firing".into(),
            ));
        }
        let desc = json!({
            "circuit": "pflow-zk-arkworks PetriTransitionCircuit (Poseidon BN254, transition multiplexer)",
            "pflow-rs": provenance::PFLOW_RS_REV,
            "places": net.display,
            "transitions": transitions,
            "inputs": inputs,
            "outputs": outputs,
        });
        let digest = provenance::sha256_hex(canonical(&desc).as_bytes());

        let mut caveats = vec![
            format!(
                "the circuit (pflow-zk-arkworks at pflow-rs {}) constrains post = pre + C·e_t on the committed markings, but range-checks neither token counts nor its enabledness remainders, so a proof alone does not show the firing was enabled; pflow-native refuses to prove a firing the metamodel rule rejects (FEATURES.md §D 9)",
                &provenance::PFLOW_RS_REV[..12]
            ),
            "state roots are unsalted Poseidon hashes of the whole marking: they bind it but do not hide a small one (anyone can hash candidates)".into(),
            "keys come from a Groth16 setup run on this machine for this circuit (trapdoor discarded); a verifier who did not run it trusts whoever did".into(),
        ];
        let gated: Vec<String> = transitions
            .iter()
            .filter(|t| !m.tests(t).is_empty())
            .cloned()
            .collect();
        if !gated.is_empty() {
            caveats.push(format!(
                "read and inhibitor arcs (on {}) gate firings without moving tokens; the circuit does not encode them",
                gated.join(", ")
            ));
        }
        let capped: Vec<String> = net
            .places
            .iter()
            .zip(&net.display)
            .filter(|(id, _)| m.place_by_id(id).is_some_and(|p| p.capacity > 0))
            .map(|(_, d)| d.clone())
            .collect();
        if !capped.is_empty() {
            caveats.push(format!(
                "place capacities (on {}) are not encoded in the circuit",
                capped.join(", ")
            ));
        }
        Ok(Self {
            net,
            transitions,
            inputs,
            outputs,
            digest,
            caveats,
        })
    }

    /// A document marking as a circuit vector.
    pub fn vector(&self, mk: &Marking) -> Vec<i64> {
        let m = self.net.model_marking(mk);
        self.net
            .places
            .iter()
            .map(|p| m.get(p).copied().unwrap_or(0))
            .collect()
    }

    pub fn transition_index(&self, id: &str) -> Result<usize> {
        self.transitions
            .iter()
            .position(|t| t == id)
            .ok_or_else(|| Error::NotFound(format!("no transition {id:?}")))
    }

    /// post − pre is exactly transition `t`'s incidence column.
    pub fn steps_by(&self, pre: &[i64], post: &[i64], t: usize) -> bool {
        let mut d = vec![0i64; pre.len()];
        for &(p, w) in &self.inputs[t] {
            d[p] -= w;
        }
        for &(p, w) in &self.outputs[t] {
            d[p] += w;
        }
        pre.iter().zip(post).zip(&d).all(|((a, b), d)| a + d == *b)
    }

    pub const STATEMENT: &'static str = "there are markings M, M' with Poseidon(M) = preStateRoot and Poseidon(M') = postStateRoot such that M' = M + C·e_t for the transition at transitionIndex, where C is this circuit's incidence matrix";
}

/// Checks a firing of `t` from `pre` against the metamodel rule (the
/// statement the prover vouches for beyond the circuit), returning the
/// marking it produces.
#[cfg_attr(not(feature = "zk"), allow(dead_code))]
fn legal_firing(doc: &Document, pre: &Marking, t: &str) -> Result<Marking> {
    crate::sim::Compiled::new(doc)?
        .fire(t, pre)
        .map_err(|e| Error::Invalid(format!("{t} cannot fire at that marking: {e}")))
}

const UNAVAILABLE: &str = "this build has no zero-knowledge prover: it is opt-in (build with `--features zk`, e.g. `make build-zk`), because the upstream circuit does not yet prove that a firing was enabled (FEATURES.md §D 9)";

/// Whether this build can prove (greys out the `zk.*` commands).
pub fn available() -> std::result::Result<(), String> {
    if cfg!(feature = "zk") {
        Ok(())
    } else {
        Err(UNAVAILABLE.into())
    }
}

#[cfg(not(feature = "zk"))]
mod imp {
    use super::*;

    fn refuse() -> Error {
        Error::Invalid(UNAVAILABLE.into())
    }
    pub fn setup(_: &Document, _: bool) -> Result<Value> {
        Err(refuse())
    }
    pub fn prove_fire(_: &Document, _: &Marking, _: &str) -> Result<Value> {
        Err(refuse())
    }
    pub fn prove_trace(_: &Document, _: &[TraceStep], _: usize) -> Result<Value> {
        Err(refuse())
    }
    pub fn verify(_: &Document, _: &Value) -> Result<Value> {
        Err(refuse())
    }
}

#[cfg(feature = "zk")]
mod imp {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::str::FromStr;
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::Instant;

    use ark_bn254::{Bn254, Fr};
    use ark_groth16::{Groth16, PreparedVerifyingKey, Proof, ProvingKey, VerifyingKey};
    use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystem};
    use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
    use ark_snark::SNARK;
    use ark_std::rand::{rngs::StdRng, SeedableRng};
    use pflow_zk::IncidenceMatrix;
    use pflow_zk_arkworks::circuit::PetriTransitionCircuit;
    use pflow_zk_arkworks::hash::poseidon_hash_native;

    use super::*;
    use crate::provenance::{b64, random_seed, unb64};

    pub struct Keys {
        pk: ProvingKey<Bn254>,
        pvk: PreparedVerifyingKey<Bn254>,
        vk_bytes: Vec<u8>,
        constraints: usize,
        setup_ms: u64,
        /// Where these keys came from: "setup", "disk".
        origin: &'static str,
    }

    fn cache() -> &'static Mutex<HashMap<String, Arc<Keys>>> {
        static C: OnceLock<Mutex<HashMap<String, Arc<Keys>>>> = OnceLock::new();
        C.get_or_init(Default::default)
    }

    fn key_dir(digest: &str) -> Option<PathBuf> {
        crate::outbound::data_dir().map(|d| d.join("zk").join(digest))
    }

    fn matrix(c: &Circuit) -> IncidenceMatrix {
        IncidenceMatrix {
            place_labels: c.net.display.clone(),
            transition_labels: c.transitions.clone(),
            inputs: c.inputs.clone(),
            outputs: c.outputs.clone(),
            num_places: c.net.places.len(),
            num_transitions: c.transitions.len(),
        }
    }

    fn root(v: &[i64]) -> Fr {
        let xs: Vec<Fr> = v.iter().map(|&x| Fr::from(x as u64)).collect();
        poseidon_hash_native(&xs)
    }

    fn instance(
        m: &IncidenceMatrix,
        pre: &[i64],
        post: &[i64],
        t: usize,
    ) -> PetriTransitionCircuit {
        let mut c = PetriTransitionCircuit::from_incidence(m, pre, post, t);
        c.pre_state_root = Some(root(pre));
        c.post_state_root = Some(root(post));
        c
    }

    fn dummy(m: &IncidenceMatrix) -> PetriTransitionCircuit {
        let zero = vec![0i64; m.num_places];
        let mut c = PetriTransitionCircuit::from_incidence(m, &zero, &zero, 0);
        c.pre_state_root = Some(Fr::from(0u64));
        c.post_state_root = Some(Fr::from(0u64));
        c
    }

    fn count_constraints(m: &IncidenceMatrix) -> usize {
        let cs = ConstraintSystem::<Fr>::new_ref();
        match dummy(m).generate_constraints(cs.clone()) {
            Ok(()) => cs.num_constraints(),
            Err(_) => 0,
        }
    }

    fn io(e: impl std::fmt::Display) -> Error {
        Error::Io(e.to_string())
    }

    fn load(dir: &std::path::Path, m: &IncidenceMatrix) -> Option<Keys> {
        let bytes = std::fs::read(dir.join("proving.key")).ok()?;
        // Our own file: skip the (slow) subgroup checks.
        let pk = ProvingKey::<Bn254>::deserialize_uncompressed_unchecked(&bytes[..]).ok()?;
        let mut vk_bytes = vec![];
        pk.vk.serialize_compressed(&mut vk_bytes).ok()?;
        let pvk = Groth16::<Bn254>::process_vk(&pk.vk).ok()?;
        Some(Keys {
            pk,
            pvk,
            vk_bytes,
            constraints: count_constraints(m),
            setup_ms: 0,
            origin: "disk",
        })
    }

    fn generate(m: &IncidenceMatrix) -> Result<Keys> {
        let start = Instant::now();
        let mut rng = StdRng::from_seed(random_seed()?);
        let (pk, vk) = Groth16::<Bn254>::circuit_specific_setup(dummy(m), &mut rng)
            .map_err(|e| Error::Invalid(format!("Groth16 setup failed: {e}")))?;
        let pvk = Groth16::<Bn254>::process_vk(&vk)
            .map_err(|e| Error::Invalid(format!("Groth16 setup failed: {e}")))?;
        let mut vk_bytes = vec![];
        vk.serialize_compressed(&mut vk_bytes).map_err(io)?;
        Ok(Keys {
            pk,
            pvk,
            vk_bytes,
            constraints: count_constraints(m),
            setup_ms: start.elapsed().as_millis() as u64,
            origin: "setup",
        })
    }

    fn save(dir: &std::path::Path, k: &Keys, c: &Circuit) -> Result<()> {
        std::fs::create_dir_all(dir).map_err(|e| Error::Io(format!("{}: {e}", dir.display())))?;
        // Write then rename, so a reader never sees half a key.
        let put = |name: &str, bytes: &[u8]| -> Result<()> {
            let tmp = dir.join(format!("{name}.tmp{}", std::process::id()));
            std::fs::write(&tmp, bytes).map_err(io)?;
            std::fs::rename(&tmp, dir.join(name)).map_err(io)
        };
        let mut pk = vec![];
        k.pk.serialize_uncompressed(&mut pk).map_err(io)?;
        put("proving.key", &pk)?;
        put("verifying.key", &k.vk_bytes)?;
        let desc = json!({
            "proofSystem": SYSTEM,
            "circuit": c.digest,
            "places": c.net.display,
            "transitions": c.transitions,
            "constraints": k.constraints,
        });
        put(
            "circuit.json",
            serde_json::to_string_pretty(&desc)
                .expect("serializable")
                .as_bytes(),
        )
    }

    /// Keys for `c`: memory, else disk, else a fresh setup (saved); and
    /// which of the three ("memory", "disk", "setup").
    fn keys(c: &Circuit, force: bool) -> Result<(Arc<Keys>, &'static str)> {
        // One setup at a time: two callers missing the cache together must
        // not each generate (and save) different keys for one circuit.
        static SETUP: Mutex<()> = Mutex::new(());
        let _guard = SETUP.lock().unwrap_or_else(|e| e.into_inner());
        if !force {
            if let Some(k) = cache().lock().expect("zk cache").get(&c.digest) {
                return Ok((k.clone(), "memory"));
            }
        }
        let m = matrix(c);
        let dir = key_dir(&c.digest);
        let loaded = if force {
            None
        } else {
            dir.as_deref().and_then(|d| load(d, &m))
        };
        let k = match loaded {
            Some(k) => k,
            None => {
                let k = generate(&m)?;
                if let Some(d) = &dir {
                    save(d, &k, c)?;
                }
                k
            }
        };
        let origin = k.origin;
        let k = Arc::new(k);
        cache()
            .lock()
            .expect("zk cache")
            .insert(c.digest.clone(), k.clone());
        Ok((k, origin))
    }

    /// Keys for a digest that already exist (memory or disk); never runs a
    /// setup.
    fn existing_keys(digest: &str) -> Option<Arc<Keys>> {
        if let Some(k) = cache().lock().expect("zk cache").get(digest) {
            return Some(k.clone());
        }
        let dir = key_dir(digest)?;
        let bytes = std::fs::read(dir.join("proving.key")).ok()?;
        let pk = ProvingKey::<Bn254>::deserialize_uncompressed_unchecked(&bytes[..]).ok()?;
        let mut vk_bytes = vec![];
        pk.vk.serialize_compressed(&mut vk_bytes).ok()?;
        let pvk = Groth16::<Bn254>::process_vk(&pk.vk).ok()?;
        let k = Arc::new(Keys {
            pk,
            pvk,
            vk_bytes,
            constraints: 0,
            setup_ms: 0,
            origin: "disk",
        });
        cache()
            .lock()
            .expect("zk cache")
            .insert(digest.to_string(), k.clone());
        Some(k)
    }

    fn setup_ms(k: &Keys, origin: &str) -> u64 {
        if origin == "setup" {
            k.setup_ms
        } else {
            0
        }
    }

    fn key_info(c: &Circuit, k: &Keys, origin: &str) -> Value {
        json!({
            "proofSystem": SYSTEM,
            "circuit": c.digest,
            "places": c.net.display,
            "transitions": c.transitions,
            "constraints": k.constraints,
            "keys": origin,
            "setupMs": setup_ms(k, origin),
            "store": key_dir(&c.digest).map_or(json!("memory only (no data directory)"), |d| json!(d)),
            "verifyingKey": b64(&k.vk_bytes),
            "statement": Circuit::STATEMENT,
            "caveats": c.caveats,
        })
    }

    pub fn setup(doc: &Document, force: bool) -> Result<Value> {
        let c = Circuit::new(doc)?;
        let (k, origin) = keys(&c, force)?;
        Ok(key_info(&c, &k, origin))
    }

    struct OneProof {
        proof: Vec<u8>,
        pre: Fr,
        post: Fr,
        ms: u64,
    }

    fn prove_one(c: &Circuit, k: &Keys, pre: &[i64], post: &[i64], t: usize) -> Result<OneProof> {
        if pre.iter().chain(post).any(|&x| x < 0) {
            return Err(Error::Invalid(
                "a marking with a negative (or ω) count cannot be proven".into(),
            ));
        }
        if !c.steps_by(pre, post, t) {
            return Err(Error::Invalid(format!(
                "the markings do not differ by {}'s incidence column",
                c.transitions[t]
            )));
        }
        let start = Instant::now();
        let m = matrix(c);
        let circuit = instance(&m, pre, post, t);
        let (pre_root, post_root) = (
            circuit.pre_state_root.unwrap(),
            circuit.post_state_root.unwrap(),
        );
        let mut rng = StdRng::from_seed(random_seed()?);
        let proof = Groth16::<Bn254>::prove(&k.pk, circuit, &mut rng)
            .map_err(|e| Error::Invalid(format!("proving failed: {e}")))?;
        let mut bytes = vec![];
        proof.serialize_compressed(&mut bytes).map_err(io)?;
        Ok(OneProof {
            proof: bytes,
            pre: pre_root,
            post: post_root,
            ms: start.elapsed().as_millis() as u64,
        })
    }

    fn public_inputs(p: &OneProof, t: usize) -> Value {
        json!({
            "preStateRoot": p.pre.to_string(),
            "postStateRoot": p.post.to_string(),
            "transitionIndex": t,
        })
    }

    pub fn prove_fire(doc: &Document, mk: &Marking, transition: &str) -> Result<Value> {
        let c = Circuit::new(doc)?;
        let t = c.transition_index(transition)?;
        let next = legal_firing(doc, mk, transition)?;
        let (k, origin) = keys(&c, false)?;
        let (pre, post) = (c.vector(mk), c.vector(&next));
        let p = prove_one(&c, &k, &pre, &post, t)?;
        Ok(json!({
            "proofSystem": SYSTEM,
            "circuit": c.digest,
            "model": provenance::model_cid(doc)?,
            "transition": transition,
            "publicInputs": public_inputs(&p, t),
            "proof": b64(&p.proof),
            "verifyingKey": b64(&k.vk_bytes),
            "statement": Circuit::STATEMENT,
            "caveats": c.caveats,
            "places": c.net.display,
            "transitions": c.transitions,
            "witness": {
                "pre": mk,
                "post": next,
                "note": "the private witness: not needed to verify; withhold it to keep the markings private",
            },
            "constraints": k.constraints,
            "timings": {"keys": origin, "setupMs": setup_ms(&k, origin), "proveMs": p.ms},
        }))
    }

    pub fn prove_trace(doc: &Document, steps: &[TraceStep], max: usize) -> Result<Value> {
        let c = Circuit::new(doc)?;
        let (start, firings) = match steps.first() {
            Some(s) if s.transition.is_empty() => (s.marking.clone(), &steps[1..]),
            Some(_) => (crate::sim::initial_marking(doc), steps),
            None => return Err(Error::BadArgs("the trace has no steps".into())),
        };
        if firings.is_empty() {
            return Err(Error::BadArgs(
                "the trace fires nothing; there is nothing to prove".into(),
            ));
        }
        if firings.len() > max {
            return Err(Error::BadArgs(format!(
                "the trace has {} firings, over max_steps {max} (≤ {MAX_TRACE_PROOFS}); proving takes a Groth16 proof per firing",
                firings.len()
            )));
        }
        let (k, origin) = keys(&c, false)?;
        let mut cur: Marking = start;
        let mut out = vec![];
        let mut total = 0u64;
        for (i, st) in firings.iter().enumerate() {
            let t = c.transition_index(&st.transition)?;
            let next = legal_firing(doc, &cur, &st.transition)
                .map_err(|e| Error::Invalid(format!("firing {}: {e}", i + 1)))?;
            let (pre, post) = (c.vector(&cur), c.vector(&next));
            if post != c.vector(&st.marking) {
                return Err(Error::Invalid(format!(
                    "firing {} ({}): the trace's marking is not the one the firing produces; check it with provenance.check_witness",
                    i + 1,
                    st.transition
                )));
            }
            let p = prove_one(&c, &k, &pre, &post, t)?;
            total += p.ms;
            out.push(json!({
                "step": i + 1,
                "transition": st.transition,
                "publicInputs": public_inputs(&p, t),
                "proof": b64(&p.proof),
            }));
            cur = next;
        }
        Ok(json!({
            "proofSystem": SYSTEM,
            "circuit": c.digest,
            "model": provenance::model_cid(doc)?,
            "steps": out,
            "verifyingKey": b64(&k.vk_bytes),
            "statement": format!("for each step, {}; consecutive steps share a state root, so the steps chain", Circuit::STATEMENT),
            "caveats": c.caveats,
            "places": c.net.display,
            "transitions": c.transitions,
            "constraints": k.constraints,
            "timings": {"keys": origin, "setupMs": setup_ms(&k, origin), "proveMs": total, "proofs": firings.len()},
        }))
    }

    fn fr(v: &Value, what: &str) -> std::result::Result<Fr, String> {
        let s = match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => return Err(format!("{what} is missing")),
        };
        Fr::from_str(&s).map_err(|_| format!("{what} is not a field element"))
    }

    /// One proof against `pvk`: Ok(verified) or why it could not be read.
    fn check_one(
        pvk: &PreparedVerifyingKey<Bn254>,
        p: &Value,
    ) -> std::result::Result<bool, String> {
        let pi = &p["publicInputs"];
        let pre = fr(&pi["preStateRoot"], "preStateRoot")?;
        let post = fr(&pi["postStateRoot"], "postStateRoot")?;
        let t = fr(&pi["transitionIndex"], "transitionIndex")?;
        let bytes = p["proof"]
            .as_str()
            .ok_or("proof bytes are missing")
            .and_then(|s| unb64(s, "proof").map_err(|_| "proof is not base64"))?;
        let proof = Proof::<Bn254>::deserialize_compressed(&bytes[..])
            .map_err(|e| format!("proof bytes do not parse as a Groth16 proof: {e}"))?;
        Groth16::<Bn254>::verify_with_processed_vk(pvk, &[pre, post, t], &proof)
            .map_err(|e| format!("verification failed: {e}"))
    }

    pub fn verify(doc: &Document, v: &Value) -> Result<Value> {
        let digest = v["circuit"]
            .as_str()
            .ok_or_else(|| Error::BadArgs("not a zk proof: no circuit digest".into()))?
            .to_string();
        let open = Circuit::new(doc).ok().map(|c| c.digest);
        let embedded = v["verifyingKey"].as_str();
        let local = existing_keys(&digest);
        let mut note = None;
        let (pvk, trusted, source) = match (local, embedded) {
            (Some(k), e)
                if e.is_none_or(|e| {
                    unb64(e, "verifyingKey").ok().as_deref() == Some(&k.vk_bytes[..])
                }) =>
            {
                (k.pvk.clone(), true, "local keys for this circuit")
            }
            (local, Some(e)) => {
                if local.is_some() {
                    note = Some("this machine holds different keys for this circuit (a later zk.setup force?); the proof was checked against the key it embeds");
                }
                let bytes = unb64(e, "verifyingKey")?;
                let vk = VerifyingKey::<Bn254>::deserialize_compressed(&bytes[..])
                    .map_err(|e| Error::BadArgs(format!("verifyingKey does not parse: {e}")))?;
                let pvk = Groth16::<Bn254>::process_vk(&vk)
                    .map_err(|e| Error::Invalid(format!("verifyingKey: {e}")))?;
                (pvk, false, "the verifying key embedded in the proof")
            }
            (None, None) => {
                return Ok(json!({
                    "verdict": provenance::UNCHECKABLE,
                    "reason": "no keys for this circuit here and no embedded verifying key",
                    "circuit": digest,
                }))
            }
            (Some(_), None) => unreachable!("matched above"),
        };
        let start = Instant::now();
        let mut failures = vec![];
        let mut checked = 0;
        if let Some(steps) = v["steps"].as_array() {
            let mut prev_post: Option<Value> = None;
            for (i, s) in steps.iter().enumerate() {
                match check_one(&pvk, s) {
                    Ok(true) => {}
                    Ok(false) => failures.push(format!(
                        "step {}: the proof does not verify against its public inputs",
                        i + 1
                    )),
                    Err(e) => failures.push(format!("step {}: {e}", i + 1)),
                }
                let pre = s["publicInputs"]["preStateRoot"].clone();
                if let Some(p) = &prev_post {
                    if *p != pre {
                        failures.push(format!("step {} does not start at the state root step {} ends at; the steps do not chain", i + 1, i));
                    }
                }
                prev_post = Some(s["publicInputs"]["postStateRoot"].clone());
                checked += 1;
            }
            if steps.is_empty() {
                failures.push("the trace proof has no steps".into());
            }
        } else {
            match check_one(&pvk, v) {
                Ok(true) => {}
                Ok(false) => {
                    failures.push("the proof does not verify against its public inputs".into())
                }
                Err(e) => failures.push(e),
            }
            checked = 1;
        }
        let ok = failures.is_empty();
        let mut out = json!({
            "verdict": if ok { provenance::VALID } else { provenance::INVALID },
            "verified": ok,
            "proofs": checked,
            "failures": failures,
            "circuit": digest,
            "matchesOpenNet": open.as_deref() == Some(digest.as_str()),
            "trustedKey": trusted,
            "keySource": source,
            "verifyMs": start.elapsed().as_millis() as u64,
        });
        if let Some(n) = note {
            out["note"] = json!(n);
        } else if !trusted {
            out["note"] = json!("checked against the key the proof carries: this shows the proof is consistent with that key, not that the key belongs to this circuit; run zk.setup on the same net to verify against local keys");
        }
        if ok {
            out["reason"] = json!(Circuit::STATEMENT);
        }
        Ok(out)
    }
}

pub use imp::{prove_fire, prove_trace, setup, verify};
