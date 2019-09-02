use crate_db::builddb::*;
use crates_index::*;
use kitchen_sink::Origin;
use lts::*;
use std::path::Path;

fn main() {
    let rustc_version =  "1.24.0";
    let date = "2018-03-28"; // good for 1.24
    // let date = "2019-05-28"; // good for 1.34

    let crates = kitchen_sink::KitchenSink::new_default().unwrap();

    let db = BuildDb::new(crates.main_cache_dir().join("builds.db")).unwrap();
    let lts = LTS::new(None);
    let old_branch = lts.cut_branch_at(date).unwrap();
    let old_repo = Path::new("/tmp/oldcratesfilter");
    lts.clone_to(&old_branch, &old_repo, false).unwrap();
    let idx = Index::new(&old_repo);

    for k in idx.crates() {
        let ver = k.latest_version();
        db.set_compat(&Origin::from_crates_io_name(ver.name()), ver.version(), rustc_version, Compat::ProbablyWorks).unwrap();
    }
}
