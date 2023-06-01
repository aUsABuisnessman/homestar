//! Output of an invocation, referenced by its invocation pointer.

use crate::db::schema::receipts;
use anyhow::anyhow;
use diesel::{
    backend::Backend,
    deserialize::{self, FromSql},
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Binary,
    sqlite::Sqlite,
    AsExpression, FromSqlRow, Insertable, Queryable,
};
use homestar_core::{
    consts,
    workflow::{prf::UcanPrf, InstructionResult, Issuer, Pointer, Receipt as InvocationReceipt},
};
use homestar_wasm::io::Arg;
use libipld::{
    cbor::DagCborCodec, cid::Cid, json::DagJsonCodec, prelude::Codec, serde::from_ipld, Ipld,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

const CID_KEY: &str = "cid";
const INSTRUCTION_KEY: &str = "instruction";
const RAN_KEY: &str = "ran";
const OUT_KEY: &str = "out";
const ISSUER_KEY: &str = "iss";
const METADATA_KEY: &str = "meta";
const PROOF_KEY: &str = "prf";
const VERSION_KEY: &str = "version";

/// Receipt for [Invocation], including it's own [Cid] and a [Cid] for an [Instruction].
///
/// `@See` [homestar_core::workflow::Receipt] for more info on some internal
/// fields.
///
/// [Invocation]: homestar_core::workflow::Invocation
/// [Instruction]: homestar_core::workflow::Instruction
#[derive(Debug, Clone, PartialEq, Queryable, Insertable)]
pub struct Receipt {
    cid: Pointer,
    ran: Pointer,
    instruction: Pointer,
    out: InstructionResult<Ipld>,
    meta: LocalIpld,
    issuer: Option<Issuer>,
    prf: UcanPrf,
    version: String,
}

impl fmt::Display for Receipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Receipt: [cid: {}, instruction: {}, ran: {}, output: {:?}, metadata: {:?}, issuer: {:?}]",
            self.cid, self.instruction, self.ran, self.out, self.meta.0, self.issuer
        )
    }
}

impl Receipt {
    /// Generate a receipt.
    pub fn new(
        cid: Cid,
        instruction: Pointer,
        invocation_receipt: &InvocationReceipt<Ipld>,
    ) -> Self {
        Self {
            cid: Pointer::new(cid),
            ran: invocation_receipt.ran().to_owned(),
            instruction,
            out: invocation_receipt.out().to_owned(),
            meta: LocalIpld(invocation_receipt.meta().to_owned()),
            issuer: invocation_receipt.issuer().to_owned(),
            prf: invocation_receipt.prf().to_owned(),
            version: consts::INVOCATION_VERSION.to_string(),
        }
    }

    /// Return a runtime [Receipt] given an [Instruction] [Pointer] and
    /// [UCAN Invocation Receipt].
    ///
    /// [Instruction]: homestar_core::workflow::Instruction
    /// [UCAN Invocation Receipt]: homestar_core::workflow::Receipt
    pub fn try_with(
        instruction: Pointer,
        invocation_receipt: &InvocationReceipt<Ipld>,
    ) -> anyhow::Result<Self> {
        let cid = Cid::try_from(invocation_receipt)?;
        Ok(Receipt::new(cid, instruction, invocation_receipt))
    }

    /// Get [Ipld] metadata on a [Receipt].
    pub fn meta(&self) -> &Ipld {
        self.meta.inner()
    }

    /// Set [Ipld] metadata on a [Receipt].
    pub fn set_meta(&mut self, meta: Ipld) {
        self.meta = LocalIpld(meta)
    }

    /// Get unique identifier of receipt.
    pub fn cid(&self) -> String {
        self.cid.to_string()
    }

    /// Get inner [Cid] as bytes.
    pub fn cid_as_bytes(&self) -> Vec<u8> {
        self.cid.cid().to_bytes()
    }

    /// Return the [Cid] of the [Receipt]'s associated [Instruction].
    ///
    /// [Instruction]: homestar_core::workflow::Instruction
    pub fn instruction(&self) -> String {
        self.instruction.to_string()
    }

    /// Get instruction [Pointer] inner [Cid] as bytes.
    pub fn instruction_cid_as_bytes(&self) -> Vec<u8> {
        self.instruction.cid().to_bytes()
    }

    /// Get [Cid] in [Receipt] as a [String].
    pub fn ran(&self) -> String {
        self.ran.to_string()
    }

    /// Get executed result/value in [Receipt] as [Ipld].
    pub fn output(&self) -> &InstructionResult<Ipld> {
        &self.out
    }

    /// Return [InstructionResult] output as [Arg] for execution.
    pub fn output_as_arg(&self) -> InstructionResult<Arg> {
        match self.out.to_owned() {
            InstructionResult::Ok(res) => InstructionResult::Ok(res.into()),
            InstructionResult::Error(res) => InstructionResult::Error(res.into()),
            InstructionResult::Just(res) => InstructionResult::Just(res.into()),
        }
    }

    /// Get executed result/value in [Receipt] as encoded Cbor.
    pub fn output_encoded(&self) -> anyhow::Result<Vec<u8>> {
        let ipld = Ipld::from(self.out.to_owned());
        DagCborCodec.encode(&ipld)
    }

    /// Return semver [Version] of [Receipt].
    pub fn version(&self) -> Result<Version, semver::Error> {
        Version::parse(&self.version)
    }

    /// Return runtime receipt as stringified Json.
    pub fn to_json(&self) -> anyhow::Result<String> {
        let encoded = DagJsonCodec.encode(&Ipld::from(self.to_owned()))?;
        let s = std::str::from_utf8(&encoded)
            .map_err(|e| anyhow!("cannot stringify encoded value: {e}"))?;
        Ok(s.to_string())
    }
}

impl TryFrom<Receipt> for Vec<u8> {
    type Error = anyhow::Error;

    fn try_from(receipt: Receipt) -> Result<Self, Self::Error> {
        let receipt_ipld = Ipld::from(receipt);
        DagCborCodec.encode(&receipt_ipld)
    }
}

impl TryFrom<Vec<u8>> for Receipt {
    type Error = anyhow::Error;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        let ipld: Ipld = DagCborCodec.decode(&bytes)?;
        ipld.try_into()
    }
}

impl From<Receipt> for InvocationReceipt<Ipld> {
    fn from(receipt: Receipt) -> Self {
        InvocationReceipt::new(
            receipt.ran,
            receipt.out,
            receipt.meta.0,
            receipt.issuer,
            receipt.prf,
        )
    }
}

impl From<&Receipt> for InvocationReceipt<Ipld> {
    fn from(receipt: &Receipt) -> Self {
        InvocationReceipt::new(
            receipt.ran.clone(),
            receipt.out.clone(),
            receipt.meta.0.clone(),
            receipt.issuer.clone(),
            receipt.prf.clone(),
        )
    }
}

impl From<Receipt> for Ipld {
    fn from(receipt: Receipt) -> Self {
        Ipld::Map(BTreeMap::from([
            (CID_KEY.into(), receipt.cid.into()),
            (RAN_KEY.into(), receipt.ran.into()),
            (INSTRUCTION_KEY.into(), receipt.instruction.into()),
            (OUT_KEY.into(), receipt.out.into()),
            (METADATA_KEY.into(), receipt.meta.0),
            (
                ISSUER_KEY.into(),
                receipt
                    .issuer
                    .map(|issuer| issuer.to_string().into())
                    .unwrap_or(Ipld::Null),
            ),
            (PROOF_KEY.into(), receipt.prf.into()),
            (VERSION_KEY.into(), receipt.version.into()),
        ]))
    }
}

impl TryFrom<Ipld> for Receipt {
    type Error = anyhow::Error;

    fn try_from(ipld: Ipld) -> Result<Self, Self::Error> {
        let map = from_ipld::<BTreeMap<String, Ipld>>(ipld)?;

        let cid = from_ipld(
            map.get(CID_KEY)
                .ok_or_else(|| anyhow!("missing {CID_KEY}"))?
                .to_owned(),
        )?;

        let ran = map
            .get(RAN_KEY)
            .ok_or_else(|| anyhow!("missing {RAN_KEY}"))?
            .try_into()?;

        let instruction = map
            .get(INSTRUCTION_KEY)
            .ok_or_else(|| anyhow!("missing {INSTRUCTION_KEY}"))?
            .try_into()?;

        let out = map
            .get(OUT_KEY)
            .ok_or_else(|| anyhow!("missing {OUT_KEY}"))?;

        let meta = map
            .get(METADATA_KEY)
            .ok_or_else(|| anyhow!("missing {METADATA_KEY}"))?;

        let issuer = map
            .get(ISSUER_KEY)
            .and_then(|ipld| match ipld {
                Ipld::Null => None,
                ipld => Some(ipld),
            })
            .and_then(|ipld| from_ipld(ipld.to_owned()).ok())
            .map(Issuer::new);

        let prf = map
            .get(PROOF_KEY)
            .ok_or_else(|| anyhow!("missing {PROOF_KEY}"))?;

        let version = from_ipld(
            map.get(VERSION_KEY)
                .ok_or_else(|| anyhow!("missing {VERSION_KEY}"))?
                .to_owned(),
        )?;

        Ok(Receipt {
            cid: Pointer::new(cid),
            ran,
            instruction,
            out: InstructionResult::try_from(out)?,
            meta: LocalIpld(meta.to_owned()),
            issuer,
            prf: UcanPrf::try_from(prf)?,
            version,
        })
    }
}

/// Wrapper-type for [Ipld] in order integrate to/from for local storage/db.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, AsExpression, FromSqlRow)]
#[diesel(sql_type = Binary)]
pub struct LocalIpld(Ipld);

impl LocalIpld {
    /// Convert into owned, inner [Ipld].
    pub fn into_inner(self) -> Ipld {
        self.0
    }

    /// Convert into referenced, inner [Ipld].
    pub fn inner(&self) -> &Ipld {
        &self.0
    }
}

impl ToSql<Binary, Sqlite> for LocalIpld
where
    [u8]: ToSql<Binary, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(DagCborCodec.encode(&self.0)?);
        Ok(IsNull::No)
    }
}

impl<DB> FromSql<Binary, DB> for LocalIpld
where
    DB: Backend,
    *const [u8]: FromSql<Binary, DB>,
{
    fn from_sql(bytes: DB::RawValue<'_>) -> deserialize::Result<Self> {
        let raw_bytes = <*const [u8] as FromSql<Binary, DB>>::from_sql(bytes)?;
        let raw_bytes: &[u8] = unsafe { &*raw_bytes };
        let decoded = DagCborCodec.decode(raw_bytes)?;
        Ok(LocalIpld(decoded))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        db::{schema, Database},
        receipt::receipts,
        settings::Settings,
        test_utils,
    };
    use diesel::prelude::*;

    #[test]
    fn invocation_into_receipt() {
        let (invocation, receipt) = test_utils::receipt::receipts();
        assert_eq!(invocation.ran().to_string(), receipt.ran());
        assert_eq!(invocation.out(), receipt.output());
        assert_eq!(invocation.meta(), &receipt.meta.0);
        assert_eq!(invocation.issuer(), &receipt.issuer);
        assert_eq!(invocation.prf(), &receipt.prf);

        let output_bytes = DagCborCodec
            .encode::<Ipld>(&invocation.out().clone().into())
            .unwrap();
        assert_eq!(output_bytes, receipt.output_encoded().unwrap());
    }

    #[test]
    fn receipt_sql_roundtrip() {
        let mut conn =
            test_utils::db::MemoryDb::setup_connection_pool(Settings::load().unwrap().node())
                .conn()
                .unwrap();
        let (_, receipt) = test_utils::receipt::receipts();

        let rows_inserted = diesel::insert_into(schema::receipts::table)
            .values(&receipt)
            .execute(&mut conn)
            .unwrap();

        assert_eq!(1, rows_inserted);

        let inserted_receipt = receipts::table.load::<Receipt>(&mut conn).unwrap();

        assert_eq!(vec![receipt.clone()], inserted_receipt);
    }

    #[test]
    fn receipt_to_json() {
        let (_, receipt) = test_utils::receipt::receipts();
        assert_eq!(
            receipt.to_json().unwrap(),
            format!(
                r#"{{"cid":{{"/":"{}"}},"instruction":{{"/":"{}"}},"iss":null,"meta":null,"out":["ok",true],"prf":[],"ran":{{"/":"{}"}},"version":"{}"}}"#,
                receipt.cid(),
                receipt.instruction(),
                receipt.ran(),
                consts::INVOCATION_VERSION
            )
        );
    }

    #[test]
    fn receipt_bytes_roundtrip() {
        let (_, receipt) = test_utils::receipt::receipts();
        let bytes: Vec<u8> = receipt.clone().try_into().unwrap();
        let from_bytes = Receipt::try_from(bytes).unwrap();

        assert_eq!(receipt, from_bytes);
    }
}