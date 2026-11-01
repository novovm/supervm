//! Bounded local initialization files. A page is only an input container; it
//! never authorizes state independently of the complete manifest and pins.
use super::super::{GenesisAllocation, GenesisConfig};
use super::*;
use std::io::Read;
use std::path::{Path, PathBuf};

const PAGE_VERSION: u8 = 3;
const PAGE_RECORDS: usize = 64;
const PAGE_BYTES: usize = super::super::MAX_STARTUP_DOCUMENT_BYTES;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InitializationPageV3 {
    version: u8,
    #[serde(default)]
    allocations: Vec<GenesisAllocation>,
    identities: Vec<UcaGenesisIdentityV2>,
    accounts: Vec<UcaGenesisAccountV2>,
    approvals: Vec<UcaGenesisApprovalV2>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UcaGenesisManifestPagesV3 {
    pub version: u8,
    pub genesis: GenesisConfig,
    pub pages: Vec<PathBuf>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApprovalPagesV3 {
    version: u8,
    pages: Vec<PathBuf>,
}

fn document(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take((PAGE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= PAGE_BYTES, "initialization page byte budget");
    Ok(bytes)
}

fn decode_page(bytes: &[u8]) -> Result<InitializationPageV3> {
    let page: InitializationPageV3 =
        serde_json::from_slice(bytes).context("decode initialization page")?;
    ensure!(
        page.version == PAGE_VERSION
            && page.allocations.len()
                + page.identities.len()
                + page.accounts.len()
                + page.approvals.len()
                <= PAGE_RECORDS,
        "initialization page version/record budget"
    );
    // The shared grant type accepts extension fields for other callers. Do
    // not silently discard one in an operator initialization document.
    type Fields = BTreeMap<String, serde::de::IgnoredAny>;
    #[derive(Deserialize)]
    struct Entry {
        grant: Fields,
    }
    #[derive(Deserialize)]
    struct Account {
        grants: Vec<Entry>,
    }
    #[derive(Deserialize)]
    struct Shape {
        accounts: Vec<Account>,
    }
    let shape: Shape = serde_json::from_slice(bytes)?;
    for (input, account) in shape.accounts.iter().zip(&page.accounts) {
        for (entry, grant) in input.grants.iter().zip(&account.grants) {
            let canonical = serde_json::to_vec(&grant.grant)?;
            let expected: Fields = serde_json::from_slice(&canonical)?;
            ensure!(
                entry.grant.keys().eq(expected.keys()),
                "unknown initialization grant field"
            );
        }
    }
    Ok(page)
}

impl UcaGenesisManifestV2 {
    /// Original compiler input, either inline or a v3 header plus bounded files.
    pub fn read_file(path: &Path) -> Result<Self> {
        let bytes = document(path)?;
        let fields: BTreeMap<String, serde::de::IgnoredAny> = serde_json::from_slice(&bytes)?;
        if fields.contains_key("pages") {
            let header: UcaGenesisManifestPagesV3 = serde_json::from_slice(&bytes)?;
            ensure!(
                header.version == PAGE_VERSION,
                "initialization manifest page version"
            );
            let mut material = UcaInitializationConfigV2 {
                identities: vec![],
                accounts: vec![],
                approvals: vec![],
                pages: header.pages,
                expected_signing_domain: [0; 32],
                expected_initial_root: [0; 32],
                expected_final_commitment: [0; 32],
            };
            let mut genesis = header.genesis;
            material.hydrate_pages_with_genesis(
                path.parent().context("manifest file directory")?,
                &mut genesis,
            )?;
            ensure!(
                material.approvals.is_empty(),
                "manifest pages must not contain approval artifacts"
            );
            Ok(Self {
                genesis,
                identities: material.identities,
                accounts: material.accounts,
            })
        } else {
            Ok(serde_json::from_slice(&bytes)?)
        }
    }

    pub fn write_pages(
        &self,
        directory: &Path,
        page_records: usize,
    ) -> Result<UcaGenesisManifestPagesV3> {
        let mut genesis = self.genesis.clone();
        let header = UcaInitializationConfigV2 {
            identities: self.identities.clone(),
            accounts: self.accounts.clone(),
            approvals: vec![],
            pages: vec![],
            expected_signing_domain: [0; 32],
            expected_initial_root: [0; 32],
            expected_final_commitment: [0; 32],
        }
        .write_pages_with_genesis(&mut genesis, directory, page_records)?;
        Ok(UcaGenesisManifestPagesV3 {
            version: PAGE_VERSION,
            genesis,
            pages: header.pages,
        })
    }
}

impl InitializationPageV3 {
    fn empty() -> Self {
        Self {
            version: PAGE_VERSION,
            allocations: vec![],
            identities: vec![],
            accounts: vec![],
            approvals: vec![],
        }
    }
}

impl UcaInitializationConfigV2 {
    /// Prepare public initialization artifacts without modifying a database.
    /// Each page is exclusively created; callers retain the original material
    /// and exact pins. One file's resource ceiling is not a total-account cap.
    pub fn write_pages(&self, directory: &Path, page_records: usize) -> Result<Self> {
        self.write_records(&[], directory, page_records)
    }

    /// Move public allocations into the same bounded files only after their
    /// complete manifest has been reviewed/approved and all writes succeeded.
    pub fn write_pages_with_genesis(
        &self,
        genesis: &mut GenesisConfig,
        directory: &Path,
        page_records: usize,
    ) -> Result<Self> {
        let result = self.write_records(&genesis.allocations, directory, page_records)?;
        genesis.allocations.clear();
        Ok(result)
    }

    fn write_records(
        &self,
        allocations: &[GenesisAllocation],
        directory: &Path,
        page_records: usize,
    ) -> Result<Self> {
        ensure!(
            self.pages.is_empty(),
            "initialization material already paged"
        );
        ensure!(
            (1..=PAGE_RECORDS).contains(&page_records),
            "initialization page record budget"
        );
        std::fs::create_dir_all(directory)?;
        let directory = directory.canonicalize()?;
        let mut pages = Vec::new();
        let mut write = |page: InitializationPageV3| -> Result<()> {
            let bytes = serde_json::to_vec(&page)?;
            ensure!(bytes.len() <= PAGE_BYTES, "initialization page byte budget");
            let path = directory.join(format!("initialization-{:06}.json", pages.len()));
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            file.write_all(&bytes)?;
            pages.push(path);
            Ok(())
        };
        for values in allocations.chunks(page_records) {
            let mut page = InitializationPageV3::empty();
            page.allocations = values.to_vec();
            write(page)?;
        }
        for values in self.identities.chunks(page_records) {
            let mut page = InitializationPageV3::empty();
            page.identities = values.to_vec();
            write(page)?;
        }
        for values in self.accounts.chunks(page_records) {
            let mut page = InitializationPageV3::empty();
            page.accounts = values.to_vec();
            write(page)?;
        }
        for values in self.approvals.chunks(page_records) {
            let mut page = InitializationPageV3::empty();
            page.approvals = values.to_vec();
            write(page)?;
        }
        Ok(Self {
            identities: vec![],
            accounts: vec![],
            approvals: vec![],
            pages,
            expected_signing_domain: self.expected_signing_domain,
            expected_initial_root: self.expected_initial_root,
            expected_final_commitment: self.expected_final_commitment,
        })
    }

    pub(crate) fn hydrate_pages_with_genesis(
        &mut self,
        base: &Path,
        genesis: &mut GenesisConfig,
    ) -> Result<()> {
        let allocations = self.hydrate_records(base)?;
        if !allocations.is_empty() {
            ensure!(
                genesis.allocations.is_empty(),
                "initialization cannot mix inline and paged allocations"
            );
            genesis.allocations = allocations;
        }
        Ok(())
    }

    fn hydrate_records(&mut self, base: &Path) -> Result<Vec<GenesisAllocation>> {
        if self.pages.is_empty() {
            return Ok(vec![]);
        }
        ensure!(
            self.identities.is_empty() && self.accounts.is_empty() && self.approvals.is_empty(),
            "initialization cannot mix inline and paged records"
        );
        let mut allocations = Vec::new();
        for path in &self.pages {
            let path: PathBuf = if path.is_absolute() {
                path.clone()
            } else {
                base.join(path)
            };
            let page = decode_page(&document(&path)?)?;
            allocations.extend(page.allocations);
            self.identities.extend(page.identities);
            self.accounts.extend(page.accounts);
            self.approvals.extend(page.approvals);
        }
        self.pages.clear();
        Ok(allocations)
    }

    pub fn read_approvals_file(path: &Path) -> Result<Vec<UcaGenesisApprovalV2>> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Input {
            Inline(Vec<UcaGenesisApprovalV2>),
            Paged(ApprovalPagesV3),
        }
        match serde_json::from_slice::<Input>(&document(path)?)? {
            Input::Inline(approvals) => Ok(approvals),
            Input::Paged(header) => {
                ensure!(
                    header.version == PAGE_VERSION,
                    "initialization approval page version"
                );
                let mut approvals = Vec::new();
                let base = path.parent().context("approval file directory")?;
                for path in header.pages {
                    let path = if path.is_absolute() {
                        path
                    } else {
                        base.join(path)
                    };
                    let page = decode_page(&document(&path)?)?;
                    ensure!(
                        page.allocations.is_empty()
                            && page.identities.is_empty()
                            && page.accounts.is_empty(),
                        "approval page contains manifest records"
                    );
                    approvals.extend(page.approvals);
                }
                Ok(approvals)
            }
        }
    }
}
