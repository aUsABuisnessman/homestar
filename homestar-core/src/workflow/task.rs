//! A [Task] is the smallest unit of work that can be requested from a UCAN.

use super::{instruction::RunInstruction, prf::UcanPrf, Pointer};
use anyhow::anyhow;
use libipld::{
    cbor::DagCborCodec,
    cid::{
        multihash::{Code, MultihashDigest},
        Cid,
    },
    prelude::Codec,
    serde::from_ipld,
    Ipld,
};
use std::collections::BTreeMap;

const DAG_CBOR: u64 = 0x71;
const RUN_KEY: &str = "run";
const CAUSE_KEY: &str = "cause";
const METADATA_KEY: &str = "meta";
const PROOF_KEY: &str = "prf";

/// Contains the [Instruction], configuration, and a possible
/// [Receipt] of the invocation that caused this task to run.
///
/// [Instruction]: super::Instruction
/// [Receipt]: super::Receipt
#[derive(Clone, Debug, PartialEq)]
pub struct Task<'a, T> {
    run: RunInstruction<'a, T>,
    cause: Option<Pointer>,
    meta: Ipld,
    prf: UcanPrf,
}

impl<'a, T> Task<'a, T>
where
    Ipld: From<T>,
    T: Clone,
{
    /// Generate a new [Task] to run, with metadata, and `prf`.
    pub fn new(run: RunInstruction<'a, T>, meta: Ipld, prf: UcanPrf) -> Self {
        Self {
            run,
            cause: None,
            meta,
            prf,
        }
    }

    /// Generate a new [Task] to execute, with metadata, given a `cause`, and
    /// `prf`.
    pub fn new_with_cause(
        run: RunInstruction<'a, T>,
        meta: Ipld,
        prf: UcanPrf,
        cause: Option<Pointer>,
    ) -> Self {
        Self {
            run,
            cause,
            meta,
            prf,
        }
    }

    /// Return a reference pointer to given [Instruction] to run.
    ///
    /// [Instruction]: super::Instruction
    pub fn run(&self) -> &RunInstruction<'_, T> {
        &self.run
    }

    /// Get [Task] metadata in [Ipld] form.
    pub fn meta(&self) -> &Ipld {
        &self.meta
    }

    /// Turn [Task] into owned [RunInstruction].
    pub fn into_instruction(self) -> RunInstruction<'a, T> {
        self.run
    }

    /// Return the [Cid] of the contained [Instruction].
    ///
    /// [Instruction]: super::Instruction
    pub fn instruction_cid(&self) -> anyhow::Result<Cid> {
        match &self.run {
            RunInstruction::Expanded(instruction) => Ok(Cid::try_from(instruction.to_owned())?),
            RunInstruction::Ptr(instruction_ptr) => Ok(instruction_ptr.cid()),
        }
    }
}

impl<T> From<Task<'_, T>> for Ipld
where
    Ipld: From<T>,
{
    fn from(task: Task<'_, T>) -> Self {
        Ipld::Map(BTreeMap::from([
            (RUN_KEY.into(), task.run.into()),
            (
                CAUSE_KEY.into(),
                task.cause.map_or(Ipld::Null, |cause| cause.into()),
            ),
            (METADATA_KEY.into(), task.meta),
            (PROOF_KEY.into(), task.prf.into()),
        ]))
    }
}

impl<T> TryFrom<Ipld> for Task<'_, T>
where
    T: From<Ipld>,
{
    type Error = anyhow::Error;

    fn try_from(ipld: Ipld) -> Result<Self, Self::Error> {
        let map = from_ipld::<BTreeMap<String, Ipld>>(ipld)?;

        Ok(Self {
            run: RunInstruction::try_from(
                map.get(RUN_KEY)
                    .ok_or_else(|| anyhow!("no `run` set"))?
                    .to_owned(),
            )?,
            cause: map
                .get(CAUSE_KEY)
                .and_then(|ipld| match ipld {
                    Ipld::Null => None,
                    ipld => Some(ipld),
                })
                .and_then(|ipld| ipld.to_owned().try_into().ok()),
            meta: map
                .get(METADATA_KEY)
                .ok_or_else(|| anyhow!("no `metadata` field set"))?
                .to_owned(),
            prf: UcanPrf::try_from(
                map.get(PROOF_KEY)
                    .ok_or_else(|| anyhow!("no proof field set"))?
                    .to_owned(),
            )?,
        })
    }
}

impl<T> TryFrom<&Ipld> for Task<'_, T>
where
    T: From<Ipld>,
{
    type Error = anyhow::Error;

    fn try_from<'a>(ipld: &Ipld) -> Result<Self, Self::Error> {
        TryFrom::try_from(ipld.to_owned())
    }
}

impl<T> TryFrom<Task<'_, T>> for Pointer
where
    Ipld: From<T>,
{
    type Error = anyhow::Error;

    fn try_from(task: Task<'_, T>) -> Result<Self, Self::Error> {
        Ok(Pointer::new(Cid::try_from(task)?))
    }
}

impl<T> TryFrom<Task<'_, T>> for Cid
where
    Ipld: From<T>,
{
    type Error = anyhow::Error;

    fn try_from(task: Task<'_, T>) -> Result<Self, Self::Error> {
        let ipld: Ipld = task.into();
        let bytes = DagCborCodec.encode(&ipld)?;
        let hash = Code::Sha3_256.digest(&bytes);
        Ok(Cid::new_v1(DAG_CBOR, hash))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{test_utils, workflow::config::Resources, Unit};

    #[test]
    fn ipld_roundtrip() {
        let config = Resources::default();
        let instruction = test_utils::workflow::instruction::<Unit>();
        let task1 = Task::new(
            RunInstruction::Expanded(instruction.clone()),
            config.clone().into(),
            UcanPrf::default(),
        );

        let ipld1 = Ipld::from(task1.clone());

        let ipld_task = Ipld::Map(BTreeMap::from([
            (
                "rsc".into(),
                Ipld::String(
                    "ipfs://bafkreidztuwoszw2dfnzufjpsjmzj67x574qcdm2autnhnv43o3t4zmh7i".into(),
                ),
            ),
            ("op".into(), Ipld::String("ipld/fun".to_string())),
            ("input".into(), Ipld::List(vec![Ipld::Bool(true)])),
            ("nnc".into(), Ipld::String("".to_string())),
        ]));

        assert_eq!(
            ipld1,
            Ipld::Map(BTreeMap::from([
                (RUN_KEY.into(), ipld_task),
                (CAUSE_KEY.into(), Ipld::Null),
                (
                    METADATA_KEY.into(),
                    Ipld::Map(BTreeMap::from([
                        ("fuel".into(), Ipld::Integer(u64::MAX.into())),
                        ("time".into(), Ipld::Integer(100_000))
                    ]))
                ),
                (PROOF_KEY.into(), Ipld::List(vec![]))
            ]))
        );

        assert_eq!(task1, ipld1.try_into().unwrap());

        let receipt = test_utils::workflow::receipt();

        let task2 = Task::new_with_cause(
            RunInstruction::Ptr::<Ipld>(instruction.try_into().unwrap()),
            config.into(),
            UcanPrf::default(),
            Some(receipt.clone().try_into().unwrap()),
        );

        let ipld2 = Ipld::from(task2.clone());

        assert_eq!(
            ipld2,
            Ipld::Map(BTreeMap::from([
                (RUN_KEY.into(), Ipld::Link(task2.instruction_cid().unwrap())),
                (
                    CAUSE_KEY.into(),
                    Ipld::Link(Cid::try_from(receipt).unwrap())
                ),
                (
                    METADATA_KEY.into(),
                    Ipld::Map(BTreeMap::from([
                        ("fuel".into(), Ipld::Integer(u64::MAX.into())),
                        ("time".into(), Ipld::Integer(100_000))
                    ]))
                ),
                (PROOF_KEY.into(), Ipld::List(vec![]))
            ]))
        );

        assert_eq!(task2, ipld2.try_into().unwrap());
    }
}