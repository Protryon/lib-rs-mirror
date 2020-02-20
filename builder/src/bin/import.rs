use crate_db::builddb::*;
use crates_index::*;
use kitchen_sink::Origin;
use lts::*;
use std::path::Path;

#[tokio::main]
async fn main() {
    let crates = kitchen_sink::KitchenSink::new_default().await.unwrap();

    let db = BuildDb::new(crates.main_cache_dir().join("builds.db")).unwrap();
    let lts = LTS::new(None);

    for (rustc_version, date) in &[("1.14.0", "2017-02-01"), ("1.19.0", "2017-08-20"), ("1.21.0", "2017-11-21"), ("1.24.0", "2018-03-28"), ("1.32.0", "2019-02-27"), ("1.34.0", "2019-05-28")] {
        let old_branch = lts.cut_branch_at(date).unwrap();
        let old_repo = Path::new("/tmp").join(format!("oldcratesfilter-{}-{}", rustc_version, date));
        lts.clone_to(&old_branch, &old_repo, false).unwrap();
        let idx = Index::new(&old_repo);

        for k in idx.crates() {
            let ver = k.latest_version();
            db.set_compat(&Origin::from_crates_io_name(ver.name()), ver.version(), rustc_version, Compat::ProbablyWorks, false).unwrap();
        }
    }
}
