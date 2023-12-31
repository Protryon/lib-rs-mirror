use kitchen_sink::SemVer;
use ahash::HashMap;
use crate_db::builddb::{Compat, RustcMinorVersion};
use regex::Regex;
use serde_derive::*;
use ahash::HashSet;
use log::{warn, info, error, debug};

pub const DIVIDER: &str = "---XBdt8MziQTMWYwcSsHz1---";

#[derive(Deserialize)]
pub struct CompilerMessageInner {
    level: String,
    message: Option<String>,
}

#[derive(Deserialize)]
pub struct CompilerMessageTarget {
    #[serde(default)]
    // kind: Vec<String>,
    edition: Option<String>,
}

#[derive(Deserialize)]
pub struct CompilerMessage {
    target: Option<CompilerMessageTarget>,
    message: Option<CompilerMessageInner>,
    reason: Option<String>,
    package_id: Option<String>,
    #[serde(default)]
    filenames: Vec<String>,
    success: Option<bool>,
}

#[derive(Default, Debug)]
pub struct Findings {
    pub crates: HashSet<(Option<RustcMinorVersion>, String, SemVer, Compat, String)>,
    pub rustc_version: Option<RustcMinorVersion>,
    pub check_time: Option<f32>,
}

pub fn parse_analyses(stdout: &str, stderr: &str) -> Vec<Findings> {
    let divider = format!("{DIVIDER}\n");

    stdout.split(&divider).zip(stderr.split(&divider))
        .filter_map(|(out, err)| parse_analysis(out, err).map_err(|e| warn!("{}", e)).ok()).collect()
}

fn parse_package_id(id: Option<&str>) -> Option<(String, SemVer)> {
    let mut parts = id?.splitn(3, ' ');
    let name = parts.next()?.to_owned();
    let ver = SemVer::parse(parts.next()?).ok()?;
    let rest = parts.next()?;
    if !rest.starts_with('(') {
        return None;
    }
    Some((name, ver))
}

const RUSTC_FEATURES_STABLE_SINCE: &[(u16, &str)] = &[
// rg  --no-filename -o '\[stable\(feature.*\]' library/ | fgrep 1. | sort -u | sed -E 's/.*feature ?= ?"(.+)", since ?= ?"1\.(..+)\..".*/(\2, "\1"),/' | sort -V | pbcopy

(17, "addr_from_into_ip"),
(17, "box_default_extra"),
(17, "box_from_c_str"),
(17, "box_from_os_str"),
(17, "box_from_path"),
(17, "box_from_slice"),
(17, "btree_range"),
(17, "collections_bound"),
(17, "collection_debug"),
(17, "cow_str_to_string_specialization"),
(17, "default_box_extra"),
(17, "default_for_pathbuf"),
(17, "frombyteswithnulerror_impls"),
(17, "ip_from_slice"),
(17, "move_cell"),
(17, "ordering_chaining"),
(17, "process_abort"),
(17, "ptr_eq"),
(17, "ptr_unaligned"),
(17, "rc_raw"),
(17, "result_expect_err"),
(17, "string_from_iter_by_ref"),
(17, "string_to_string_specialization"),
(17, "vec_deque_partial_eq_slice"),
(18, "binary_heap_peek_mut_pop"),
(18, "c_string_from_box"),
(18, "os_string_from_box"),
(18, "path_buf_from_box"),
(18, "peek"),
(18, "process_try_wait"),
(18, "retain_hash_collection"),
(18, "string_from_box"),
(18, "vec_from_box"),
(19, "boxed_str_conv"),
(19, "command_envs"),
(19, "eprint"),
(19, "herd_cows"),
(19, "mutexguard"),
(19, "osstring_shrink_to_fit"),
(19, "reverse_cmp_key"),
(19, "thread_id"),
(19, "vec_from_mut"),
(20, "as_c_str"),
(20, "box_from_c_string"),
(20, "box_from_os_string"),
(20, "box_from_path_buf"),
(20, "box_from_str"),
(20, "box_from_vec"),
(20, "char_escape_debug"),
(20, "char_from_str"),
(20, "compile_error_macro"),
(20, "float_bits_conv"),
(20, "into_boxed_c_str"),
(20, "into_boxed_os_str"),
(20, "into_boxed_path"),
(20, "manually_drop"),
(20, "more_io_inner_methods"),
(20, "option_entry"),
(20, "sort_unstable"),
(20, "stdio_from"),
(20, "std_guard_impls"),
(20, "str_box_extras"),
(20, "str_checked_slicing"),
(20, "str_mut_extras"),
(20, "utf8_error_error_len"),
(21, "asraw_stdio"),
(21, "compiler_fences"),
(21, "discriminant_value"),
(21, "iterator_for_each"),
(21, "needs_drop"),
(21, "ord_max_min"),
(21, "shared_from_slice"),
(21, "tcpstream_connect_timeout"),
(21, "vec_splice"),
(22, "cow_box_error"),
(22, "indirect_hasher_impl"),
(22, "op_assign_builtins_by_ref"),
(23, "ascii_methods_on_intrinsics"),
(23, "atomic_from"),
(23, "rwlock_guard_sync"),
(23, "unit_from_iter"),
(24, "ascii_ctype_on_intrinsics"),
(24, "atomic_bool_from"),
(24, "atomic_pointer"),
(24, "mpsc_error_conversions"),
(24, "mutex_from"),
(24, "refcell_replace"),
(24, "refcell_swap"),
(24, "rw_lock_from"),
(24, "shared_from_slice2"),
(24, "spin_loop_hint"),
(25, "cursor_mut_vec"),
(25, "duration_core"),
(25, "nonnull"),
(25, "panic_col"),
(25, "path_component_asref"),
(26, "box_leak"),
(26, "core_ascii"),
(26, "entry_and_modify"),
(26, "env_unimpl_send_sync"),
(26, "from_utf8_error_as_bytes"),
(26, "fs_read_write"),
(26, "fs_read_write_bytes"),
(26, "fused"),
(26, "getpid"),
(26, "i128"),
(26, "i128"),
(26, "inclusive_range"),
(26, "lossless_iusize_conv"),
(26, "panic_hook_display"),
(26, "pointer_methods"),
(26, "slice_rotate"),
(26, "string_retain"),
(26, "thread_local_try_with"),
(27, "core_hint"),
(27, "duration_debug_impl"),
(27, "duration_extras"),
(27, "duration_from_micros"),
(27, "hash_map_remove_entry"),
(27, "inclusive_range_methods"),
(27, "iterator_try_fold"),
(27, "iter_rfind"),
(27, "iter_rfold"),
(27, "nonnull_cast"),
(27, "option_filter"),
(27, "simd_arch"),
(27, "simd_x86"),
(27, "slice_rsplit"),
(27, "splice"),
(27, "swap_nonoverlapping"),
(27, "swap_with_slice"),
(27, "take_set_limit"),
(27, "unix_ppid"),
(27, "unreachable"),
(28, "alloc_layout"),
(28, "alloc_module"),
(28, "alloc_system_type"),
(28, "any_send_sync_methods"),
(28, "assoc_unix_epoch"),
(28, "collections_range"),
(28, "cow_from_cstr"),
(28, "cow_from_osstr"),
(28, "cow_from_pathbuf_ref"),
(28, "cow_from_string_ref"),
(28, "cow_from_vec_ref"),
(28, "cstring_from_cow_cstr"),
(28, "default_mut_str"),
(28, "entry_or_default"),
(28, "extend_for_unit"),
(28, "fmt_flags_align"),
(28, "from_bool"),
(28, "from_ref"),
(28, "global_allocator"),
(28, "global_alloc"),
(28, "iterator_repeat_with"),
(28, "iterator_step_by"),
(28, "nonzero"),
(28, "osstring_from_cow_osstr"),
(28, "pathbuf_from_cow_path"),
(28, "path_ancestors"),
(28, "slice_get_slice"),
(29, "iterator_flatten"),
(29, "joinhandle_impl_send_sync"),
(29, "more_box_slice_clone"),
(29, "never_hash"),
(29, "os_str_str_ref_eq"),
(29, "proc_macro_lib2"),
(29, "rc_downcast"),
(30, "core_c_void"),
(30, "error_source"),
(30, "ip_constructors"),
(30, "iterator_find_map"),
(30, "option_ref_from_ref_option"),
(30, "slice_align_to"),
(30, "token_stream_extend"),
(30, "trim_direction"),
(31, "chunks_exact"),
(31, "from_nonzero"),
(31, "option_replace"),
(31, "rchunks"),
(31, "symmetric_u32_duration_mul"),
(32, "boxed_slice_from_iter"),
(32, "dbg_macro"),
(32, "int_to_from_bytes"),
(32, "path_from_str"),
(33, "convert_id"),
(33, "duration_as_u128"),
(33, "pin"),
(33, "rw_exact_all_at"),
(33, "simd_wasm32"),
(33, "simd_x86_adx"),
(33, "transpose_result"),
(33, "vec_resize_with"),
(34, "convert_infallible"),
(34, "get_type_id"),
(34, "integer_atomics_stable"),
(34, "iter_from_fn"),
(34, "iter_successors"),
(34, "no_panic_pow"),
(34, "process_pre_exec"),
(34, "signed_nonzero"),
(34, "slice_sort_by_cached_key"),
(34, "split_ascii_whitespace"),
(34, "str_escape"),
(34, "time_checked_add"),
(34, "try_from"),
(35, "asraw_stdio_locks"),
(35, "boxed_closure_impls"),
(35, "copied"),
(35, "copysign"),
(35, "exact_size_case_mapping_iter"),
(35, "from_ref_string"),
(35, "nonzero_parse"),
(35, "ptr_hash"),
(35, "range_contains"),
(35, "refcell_map_split"),
(35, "refcell_replace_swap"),
(35, "wasi_ext_doc"),
(36, "align_offset"),
(36, "alloc"),
(36, "core_array"),
(36, "futures_api"),
(36, "hashbrown"),
(36, "iovec"),
(36, "iter_copied"),
(36, "maybe_uninit"),
(36, "string_borrow_mut"),
(36, "str_as_mut_ptr"),
(36, "try_from_slice_error"),
(36, "vecdeque_rotate"),
(37, "as_cell"),
(37, "borrow_state"),
(37, "bufreader_buffer"),
(37, "copy_within"),
(37, "iter_arith_traits_option"),
(37, "iter_nth_back"),
(37, "option_xor"),
(37, "reverse_bits"),
(37, "shared_from_iter"),
(37, "unreachable_wasm32"),
(37, "vec_as_ptr"),
(38, "builtin_macro_prelude"),
(38, "chars_debug_impl"),
(38, "double_ended_peek_iterator"),
(38, "double_ended_step_by_iterator"),
(38, "double_ended_take_iterator"),
(38, "duration_float"),
(38, "euclidean_division"),
(38, "pin_raw"),
(38, "ptr_cast"),
(38, "type_name"),
(39, "ascii_escape_display"),
(39, "checked_duration_since"),
(39, "pin_into_inner"),
(39, "weak_ptr_eq"),
(39, "wrapping_ref_ops"),
(40, "array_value_iter_impls"),
(40, "float_to_from_bytes"),
(40, "map_get_key_value"),
(40, "mem_take"),
(40, "option_deref"),
(40, "option_flattening"),
(40, "repeat_generic_slice"),
(40, "todo_macro"),
(40, "udp_peer_addr"),
(41, "core_panic_info"),
(41, "maybe_uninit_debug"),
(41, "nz_int_conv"),
(41, "pin_trait_impls"),
(41, "result_map_or"),
(41, "result_map_or_else"),
(41, "weak_counts"),
(42, "debug_map_key_value"),
(42, "integer_exp_format"),
(42, "iter_empty_send_sync"),
(42, "manually_drop_take"),
(42, "matches_macro"),
(42, "slice_from_raw_parts"),
(42, "wait_timeout_until"),
(42, "wait_until"),
(43, "assoc_int_consts"),
(43, "boxed_slice_try_from"),
(43, "core_primitive"),
(43, "cstring_from_vec_of_nonzerou8"),
(43, "extra_log_consts"),
(43, "iter_once_with"),
(43, "once_is_completed"),
(43, "string_as_mut"),
(44, "alloc_layout_manipulation"),
(44, "convert_infallible_hash"),
(44, "float_approx_unchecked_to"),
(44, "from_mut_str_for_string"),
(44, "iovec-send-sync"),
(44, "mut_osstr"),
(44, "path_buf_capacity"),
(44, "proc_macro_lexerror_impls"),
(44, "vec_from_array"),
(45, "atomic_min_max"),
(45, "box_from_array"),
(45, "box_from_cow"),
(45, "box_str2"),
(45, "btreemap_remove_entry"),
(45, "nonzero_bitor"),
(45, "no_more_cas"),
(45, "osstring_from_str"),
(45, "process_set_argv0"),
(45, "proc_macro_mixed_site"),
(45, "proc_macro_span_located_at"),
(45, "proc_macro_span_resolved_at"),
(45, "proc_macro_token_stream_default"),
(45, "rc_as_ptr"),
(45, "saturating_neg"),
(45, "shared_from_cow"),
(45, "socketaddr_ordering"),
(45, "str_strip"),
(45, "unicode_version"),
(45, "weak_into_raw"),
(46, "buffered_io_capacity"),
(46, "char_to_string_specialization"),
(46, "from_char_for_string"),
(46, "leading_trailing_ones"),
(46, "nzint_try_from_int_conv"),
(46, "option_zip_option"),
(46, "partialeq_vec_for_ref_slice"),
(46, "simd_x86_mm_loadu_si64"),
(46, "string_u16_to_socket_addrs"),
(46, "track_caller"),
(46, "vec_drain_as_slice"),
(46, "vec_intoiter_as_ref"),
(47, "cstr_range_from"),
(47, "inner_deref"),
(47, "proc_macro_raw_ident"),
(47, "ptr_offset_from"),
(47, "range_is_empty"),
(47, "tau_constant"),
(47, "vec_leak"),
(48, "array_try_from_vec"),
(48, "deque_make_contiguous"),
(48, "future_readiness_fns"),
(48, "partialeq_vec_for_slice"),
(48, "raw_fd_reflexive_traits"),
(48, "slice_ptr_range"),
(48, "write_mt"),
(49, "nzint_try_from_nzint_conv"),
(49, "renamed_spin_loop"),
(49, "slice_select_nth_unstable"),
(50, "alloc_layout_error"),
(50, "clamp"),
(50, "index_trait_on_arrays"),
(50, "lazy_bool_to_option"),
(50, "or_insert_with_key"),
(50, "proc_macro_punct_eq"),
(50, "refcell_take"),
(50, "slice_fill"),
(50, "unsafe_cell_get_mut"),
(51, "arc_mutate_strong_count"),
(51, "array_value_iter"),
(51, "as_mut_str_for_str"),
(51, "box_send_sync_any_downcast"),
(51, "deque_range"),
(51, "empty_seek"),
(51, "error_by_ref"),
(51, "iterator_fold_self"),
(51, "more_char_conversions"),
(51, "nonzero_div"),
(51, "once_poison"),
(51, "panic_any"),
(51, "peekable_next_if"),
(51, "poll_map"),
(51, "raw_ref_macros"),
(51, "seek_convenience"),
(51, "slice_fill_with"),
(51, "slice_strip"),
(51, "split_inclusive"),
(51, "unsigned_abs"),
(51, "wake_trait"),
(52, "arc_error"),
(52, "assoc_char_consts"),
(52, "assoc_char_funcs"),
(52, "fmt_as_str"),
(52, "osstring_extend"),
(52, "partition_point"),
(52, "proc_macro_punct_eq_flipped"),
(52, "str_split_once"),
(53, "array_from_ref"),
(53, "array_into_iter_impl"),
(53, "atomic_fetch_update"),
(53, "btree_retain"),
(53, "bufreader_seek_relative"),
(53, "cmp_min_max_by"),
(53, "debug_non_exhaustive"),
(53, "duration_saturating_ops"),
(53, "duration_zero"),
(53, "int_bits_const"),
(53, "is_subnormal"),
(53, "nonzero_leading_trailing_zeros"),
(53, "option_insert"),
(53, "ordering_helpers"),
(53, "osstring_ascii"),
(53, "peekable_peek_mut"),
(53, "rc_mutate_strong_count"),
(53, "slice_index_with_ops_bound_pair"),
(53, "slice_iter_mut_as_slice"),
(53, "split_inclusive"),
(53, "unsupported_error"),
(53, "vec_extend_from_within"),
(54, "i8_to_string_specialization"),
(54, "map_into_keys_values"),
(54, "out_of_memory_error"),
(54, "proc_macro_literal_parse"),
(54, "u8_to_string_specialization"),
(54, "vecdeque_binary_search"),
(54, "wasm_simd"),
(55, "array_map"),
(55, "bound_cloned"),
(55, "control_flow_enum_type"),
(55, "int_error_matching"),
(55, "io_into_inner_error_parts"),
(55, "maybe_uninit_ref"),
(55, "maybe_uninit_write"),
(55, "prelude_2015"),
(55, "prelude_2018"),
(55, "prelude_2021"),
(55, "proc_macro_group_span"),
(55, "seek_rewind"),
(55, "simd_x86_bittest"),
(55, "string_drain_as_str"),
(56, "bufwriter_into_parts"),
(56, "extend_for_tuple"),
(56, "shrink_to"),
(56, "std_collections_from_array"),
(56, "unix_chroot"),
(56, "unsafe_cell_raw_get"),
(57, "array_as_slice"),
(57, "command_access"),
(57, "is_symlink"),
(57, "iter_map_while"),
(57, "proc_macro_is_available"),
(57, "try_reserve"),
(58, "copy_clone_array_lib"),
(58, "cstring_from_vec_with_nul"),
(58, "is_symlink"),
(58, "option_result_unwrap_unchecked"),
(58, "rc_ref_unwind_safe"),
(58, "saturating_div"),
(58, "unix_process_wait_more"),
(58, "with_options"),
(59, "asm"),
(59, "available_parallelism"),
(59, "case_mapping_double_ended"),
(59, "control_flow_enum_is"),
(59, "entry_insert"),
(59, "global_asm"),
(59, "iter_zip"),
(59, "neon_intrinsics"),
(59, "nonzero_is_power_of_two"),
(59, "result_cloned"),
(59, "result_copied"),
(59, "sync_once_unwind_safe"),
(59, "try_from_mut_slice_to_array"),
(59, "u8_from_char"),
(60, "arc_new_cyclic"),
(60, "inherent_ascii_escape"),
(60, "int_abs_diff"),
(60, "io_errorkind_display"),
(55, "maybe_uninit_extra"), // .write used to belong to it, and was partially stabilized
(60, "arc_new_cyclic"),
(60, "inherent_ascii_escape"),
(60, "int_abs_diff"),
(60, "io_errorkind_display"),
(60, "maybe_uninit_extra"),
(60, "not_never"),
(60, "simd_aarch64"),
(60, "vec_spare_capacity"),
(60, "wrapping_int_assign_impl"),
(61, "cursor_array"),
(61, "pin_static_ref"),
(61, "process_exitcode"),
(61, "termination_trait_lib"),
(61, "thread_is_running"),
(61, "unix_socket_creation"),
(61, "vec_retain_mut"),
(62, "assertunwindsafe_default"),
(62, "bool_to_option"),
(62, "encode_wide_fused_iterator"),
(62, "rdm_intrinsics"),
(62, "shared_from_str"),
(62, "stdin_forwarders"),
(62, "total_cmp"),
(62, "windows_process_extensions_raw_arg"),
(63, "array_from_fn"),
(63, "asrawfd_ptrs"),
(63, "box_into_pin"),
(63, "cell_filter_map"),
(63, "io_safety"),
(63, "ipv6_to_ipv4_mapped"),
(63, "path_try_exists"),
(63, "scoped_threads"),
(63, "slice_ptr_len_nonnull"),
(63, "toowned_clone_into"),
(63, "try_reserve_2"),
(63, "vecdeque_read_write"),
(64, "alloc_c_string"),
(64, "alloc_ffi"),
(64, "asfd_ptrs"),
(64, "btree_unwindsafe"),
(64, "core_c_str"),
(64, "core_ffi_c"),
(64, "future_poll_fn"),
(64, "into_future"),
(64, "nonzero_checked_ops"),
(64, "os_string_fmt_write"),
(64, "process_set_process_group"),
(64, "ready_macro"),
(64, "tcp_listener_incoming_fused_iterator"),
(64, "windows_file_type_ext"),
(65, "backtrace"),
(65, "bound_as_ref_shared"),
(65, "io_read_to_string"),
(65, "io_safety_wasi"),
(65, "ptr_const_cast"),
(66, "bench_black_box"),
(66, "boxed_array_try_from_vec"),
(66, "duration_checked_float"),
(66, "map_first_last"),
(66, "mixed_integer_ops"),
(66, "proc_macro_source_text"),
(66, "unzip_option"),
];

fn parse_analysis(stdout: &str, stderr: &str) -> Result<Findings, String> {
    let stdout = stdout.trim();
    if stdout.is_empty() {
        return Err("no input".into());
    }

    let feature_flags: HashMap<_,_> = RUSTC_FEATURES_STABLE_SINCE.iter().map(|&(v, k)| (k, v)).collect();

    let mut findings = Findings::default();
    let mut some_deps_broken = false;
    let user_time = Regex::new(r"^user\s+(\d+)m(\d+\.\d+)s$").expect("regex");

    let mut lines = stdout.split('\n');
    let first_line = lines.next().ok_or("no first line!?")?;
    let mut fl = first_line.split(' ');
    if fl.next().unwrap() != "CHECKING" {
        return Err(format!("----------\nBad first line {first_line}"));
    }
    let rustc_version_semver = SemVer::parse(fl.next().ok_or("first line 1")?).map_err(|e| e.to_string())?;
    assert_eq!(rustc_version_semver.major, 1);
    findings.rustc_version = Some(rustc_version_semver.minor as u16);
    let top_level_crate_name = fl.next().ok_or("no top-level crate name")?;
    let top_level_crate_ver = SemVer::parse(fl.next().ok_or("first line 2")?).map_err(|e| e.to_string())?;

    let mut printed = HashSet::default();
    for line in lines.filter(|l| l.starts_with('{')) {
        let line = line
            .trim_start_matches("unknown line ")
            .trim_start_matches("failure-note ")
            .trim_start_matches("compiler-message ");

        if let Ok(msg) = serde_json::from_str::<CompilerMessage>(line) {
            let desc = msg.message.as_ref().and_then(|m| m.message.as_deref());

            if let Some((name, ver)) = parse_package_id(msg.package_id.as_deref()) {
                if name == "______" || name == "_____" || name == "build-script-build" {
                    continue;
                }

                let level = msg.message.as_ref().map(|m| m.level.as_str()).unwrap_or("");
                let reason = msg.reason.as_deref().unwrap_or("");
                // not an achievement, ignore
                if msg.filenames.iter().any(|f| f.contains("/build-script-build")) {
                    continue;
                }


                debug!("{}@{} > {}@{}: {} {} {}", top_level_crate_name, top_level_crate_ver, name, ver, level, desc.unwrap_or(""), reason);

                if let Some(desc) = desc {
                    if desc.starts_with("couldn't read /") ||
                        desc.starts_with("Current directory is invalid:") ||
                        desc.starts_with("file not found for module") ||
                        desc.starts_with("could not parse/generate dep") ||
                        desc.contains("No such file or directory") ||
                        desc.starts_with("error: could not parse/generate dep") ||
                        desc.starts_with("couldn't create a temp dir:") {
                        return Err(format!("• err: broken build, ignoring: {desc}")); // oops, our bad
                    }

                    if desc.starts_with("`#![feature]` may not be used on the stable release channel") {
                        return Err("Nightly crate (or unstable features enabled), can't check".to_string());
                    }

                    if let Some(feat) = desc.strip_prefix("use of unstable library feature '") {
                        let feat = feat.split('\'').next().unwrap();
                        if let Some(rustc_min) = feature_flags.get(feat) {
                            info!("found feature {} >= {} ({} {})", feat, rustc_min, name, ver);
                            // if testing the crate by itself, then there are no other uses (like non-default feature flags) that could break it.
                            // reports from nested depdendencies may be unreliable.
                            findings.crates.insert((Some(rustc_min-1), name.clone(), ver.clone(), if name == top_level_crate_name { Compat::DefinitelyIncompatible } else { Compat::LikelyIncompatible }, desc.into()));
                        } else {
                            info!("• err: unknown feature !? {}", feat);
                        }
                    }
                    else if desc.starts_with("associated constants are experimental") {
                        findings.crates.insert((Some(19), name.clone(), ver.clone(), Compat::LikelyIncompatible, desc.into()));
                    }
                    else if desc.starts_with("no method named `trim_start`") ||
                        desc.starts_with("`crate` in paths is experimental") ||
                        desc.starts_with("no method named `trim_start_matches` found for type `std::") {
                        findings.crates.insert((Some(29), name.clone(), ver.clone(), Compat::LikelyIncompatible, desc.into()));
                    }
                    else if desc.starts_with("use of unstable library feature 'split_ascii_whitespace") ||
                        desc.starts_with("unresolved import `core::convert::Infallible`") ||
                        desc.starts_with("cannot find type `NonZeroI") ||
                        desc.starts_with("cannot find trait `TryFrom` in this") ||
                        desc.starts_with("use of unstable library feature 'const_integer_atomics") {
                        findings.crates.insert((Some(33), name.clone(), ver.clone(), Compat::LikelyIncompatible, desc.into()));
                    }
                    else if desc.starts_with("cannot find trait `Unpin` in this scope") ||
                        desc.starts_with("use of unstable library feature 'pin'") {
                        findings.crates.insert((Some(32), name.clone(), ver.clone(), Compat::SuspectedIncompatible, desc.into()));
                    }
                    else if desc.starts_with("const fn is unstable") {
                        findings.crates.insert((Some(30), name.clone(), ver.clone(), Compat::SuspectedIncompatible, desc.into()));
                    }
                    else if desc.starts_with("use of unstable library feature 'int_to_from_bytes") ||
                        desc.starts_with("`core::mem::size_of` is not yet stable as a const fn") ||
                        desc.contains(">::from_be` is not yet stable as a const fn") ||
                        desc.contains(">::to_be` is not yet stable as a const fn") ||
                        desc.contains(">::to_le` is not yet stable as a const fn") ||
                        desc.contains(">::from_le` is not yet stable as a const fn") {
                        findings.crates.insert((Some(31), name.clone(), ver.clone(), Compat::LikelyIncompatible, desc.into()));
                    }
                    else if desc.starts_with("unresolved import `std::ops::RangeBounds`") ||
                        desc.starts_with("the `#[repr(transparent)]` attribute is experimental") ||
                        desc.starts_with("unresolved import `std::alloc::Layout") {
                        findings.crates.insert((Some(27), name.clone(), ver.clone(), Compat::SuspectedIncompatible, desc.into()));
                    }
                    else if desc.starts_with("no method named `align_to` found for type `&") ||
                        desc.starts_with("no method named `trim_end` found for type `&str`") ||
                        desc.starts_with("scoped attribute `rustfmt::skip` is experimental") {
                        findings.crates.insert((Some(29), name.clone(), ver.clone(), Compat::SuspectedIncompatible, desc.into()));
                    }
                    else if desc.starts_with("`dyn Trait` syntax is unstable") ||
                        desc.starts_with("unresolved import `self::std::hint`") ||
                        desc.starts_with("`cfg(target_feature)` is experimental and subject") {
                        findings.crates.insert((Some(26), name.clone(), ver.clone(), Compat::LikelyIncompatible, desc.into()));
                    }
                    else if desc.starts_with("128-bit type is unstable") ||
                        desc.starts_with("128-bit integers are not stable") ||
                        desc.starts_with("underscore lifetimes are unstable") ||
                        desc.starts_with("`..=` syntax in patterns is experimental") ||
                        desc.starts_with("inclusive range syntax is experimental") {
                        findings.crates.insert((Some(25), name.clone(), ver.clone(), Compat::LikelyIncompatible, desc.into()));
                    }
                    else if desc.starts_with("unresolved import `std::ptr::NonNull`") {
                        findings.crates.insert((Some(24), name.clone(), ver.clone(), Compat::SuspectedIncompatible, desc.into()));
                    }
                    else if desc.starts_with("use of unstable library feature 'maybe_uninit'") ||
                        desc.starts_with("no function or associated item named `uninit` found for type `core::me") ||
                        desc.starts_with("no function or associated item named `uninit` found for type `std::me") ||
                        desc.starts_with("cannot find type `IoSliceMut`") ||
                        desc.starts_with("failed to resolve: could not find `IoSliceMut` in") ||
                        desc.starts_with("cannot find type `Context` in module `core::task") ||
                        desc.starts_with("unresolved import `core::task::Context`") ||
                        desc.starts_with("no method named `assume_init` found for type `core::mem") ||
                        desc.starts_with("no method named `assume_init` found for type `std::mem") ||
                        desc.starts_with("unresolved import `std::task::Context`") ||
                        desc.starts_with("unresolved imports `io::IoSlice") ||
                        desc.starts_with("unresolved import `std::io::IoSlice") {
                        findings.crates.insert((Some(35), name.clone(), ver.clone(), Compat::SuspectedIncompatible, desc.into()));
                    }
                    else if desc.starts_with("use of unstable library feature 'matches_macro'") ||
                        desc.starts_with("cannot find macro `matches!`") ||
                        desc.starts_with("cannot find macro `matches` in") ||
                        desc.starts_with("no associated item named `MAX` found for type `u") ||
                        desc.starts_with("no associated item named `MIN` found for type `u") {
                        findings.crates.insert((Some(41), name.clone(), ver.clone(), Compat::SuspectedIncompatible, desc.into()));
                    }
                    else if desc.starts_with("arbitrary `self` types are unstable") ||
                        desc.contains("type of `self` without the `arbitrary_self_types`") ||
                        desc.contains("no method named `map_or` found for type `std::result::Result") ||
                        desc.contains("unexpected `self` parameter in function") {
                        findings.crates.insert((Some(40), name.clone(), ver.clone(), Compat::LikelyIncompatible, desc.into()));
                    }
                    else if desc.starts_with("no associated item named `MAX` found for type `u") ||
                        desc.starts_with("attributes are not yet allowed on `if` expressions") ||
                        desc.starts_with("no associated item named `INFINITY` found for type `f") ||
                        desc.starts_with("no associated item named `NEG_INFINITY` found for type `f") ||
                        desc.starts_with("no associated item named `NAN` found for type `f") ||
                        desc.starts_with("no associated item named `MIN` found for type `i") ||
                        desc.starts_with("no associated item named `MIN` found for type `u") ||
                        desc.starts_with("no associated item named `MAX` found for type `i") {
                        findings.crates.insert((Some(42), name.clone(), ver.clone(), Compat::SuspectedIncompatible, desc.into()));
                    }
                    else if desc.starts_with("no method named `strip_prefix` found for type `&str`") {
                        findings.crates.insert((Some(44), name.clone(), ver.clone(), Compat::LikelyIncompatible, desc.into()));
                    }
                    else if desc.starts_with("use of unstable library feature 'inner_deref'") ||
                        desc.starts_with("arrays only have std trait implementations for lengths 0..=32") {
                        findings.crates.insert((Some(46), name.clone(), ver.clone(), Compat::LikelyIncompatible, desc.into()));
                    }
                    else if desc.starts_with("#[doc(alias = \"...\")] is experimental") {
                        findings.crates.insert((Some(47), name.clone(), ver.clone(), Compat::LikelyIncompatible, desc.into()));
                    }
                    else if desc.starts_with("unions with non-`Copy` fields are unstable") {
                        findings.crates.insert((Some(48), name.clone(), ver.clone(), Compat::LikelyIncompatible, desc.into()));
                    }
                    else if desc.starts_with("const generics are unstable") ||
                        desc.starts_with("const generics in any position are currently unsupported") {
                        findings.crates.insert((Some(49), name.clone(), ver.clone(), Compat::LikelyIncompatible, desc.into()));
                    }
                    else if desc.starts_with("no method named `fill_with` found for mutable reference `&mut [") {
                        findings.crates.insert((Some(50), name.clone(), ver.clone(), Compat::SuspectedIncompatible, desc.into()));
                    }
                    else if desc.starts_with("there is no argument named `") {
                        findings.crates.insert((Some(57), name.clone(), ver.clone(), Compat::LikelyIncompatible, desc.into()));
                    }
                    else if desc.starts_with("type parameters must be declared prior to const parameters") {
                        findings.crates.insert((Some(58), name.clone(), ver.clone(), Compat::LikelyIncompatible, desc.into()));
                    }
                    else if desc.starts_with("the `#[track_caller]` attribute is an experimental") ||
                        desc.starts_with("`while` is not allowed in a `const fn`") ||
                        desc.starts_with("`while` is not allowed in a `const`") ||
                        desc.starts_with("`if` is not allowed in a `const fn`") ||
                        desc.starts_with("loops and conditional expressions are not stable in const fn") ||
                        desc.starts_with("loops are not allowed in const fn") ||
                        desc.starts_with("`if`, `match`, `&&` and `||` are not stable in const fn") ||
                        desc.starts_with("`match` is not allowed in a `const fn`") {
                        findings.crates.insert((Some(45), name.clone(), ver.clone(), Compat::LikelyIncompatible, desc.into()));
                    }
                    else if desc.starts_with("use of unstable library feature 'ptr_cast") ||
                       desc.starts_with("unresolved import `core::any::type_name") ||
                       desc.starts_with("unresolved import `std::any::type_name") ||
                       desc.starts_with("cannot find function `type_name` in module `core::any`") ||
                       desc.starts_with("cannot find function `type_name` in module `std::any") ||
                       desc.starts_with("no method named `cast` found for type `*") ||
                       desc.starts_with("use of unstable library feature 'euclidean_division") {
                        findings.crates.insert((Some(37), name.clone(), ver.clone(), Compat::SuspectedIncompatible, desc.into()));
                    }
                    else if desc.starts_with("use of unstable library feature 'option_flattening") ||
                        desc.starts_with("cannot find function `take` in module `mem") ||
                        desc.starts_with("subslice patterns are unstable") ||
                        desc.starts_with("no method named `to_ne_bytes` found for type") ||
                        desc.starts_with("no method named `to_be_bytes` found for type") ||
                        desc.starts_with("no function or associated item named `from_ne_bytes`") ||
                        desc.starts_with("no function or associated item named `from_be_bytes`") ||
                        desc.starts_with("cannot find macro `todo!` in this scope") ||
                        desc.starts_with("no method named `as_deref` found for type") ||
                        desc.starts_with("cannot find function `take` in module `std::mem") ||
                        desc.starts_with("`cfg(doctest)` is experimental and subject to change") ||
                        desc.starts_with("the `#[non_exhaustive]` attribute is an experimental") ||
                        desc.starts_with("syntax for subslices in slice patterns is not yet stabilized") ||
                        desc.starts_with("non exhaustive is an experimental feature") {
                        findings.crates.insert((Some(39), name.clone(), ver.clone(), Compat::SuspectedIncompatible, desc.into()));
                    }
                    else if desc.starts_with("cannot bind by-move into a pattern") ||
                        desc.starts_with("async/await is unstable") ||
                        desc.starts_with("async blocks are unstable") ||
                        desc.starts_with("`await` is a keyword in the 2018 edition") ||
                        desc.starts_with("async fn is unstable") {
                        findings.crates.insert((Some(38), name.clone(), ver.clone(), Compat::LikelyIncompatible, desc.into()));
                    }
                    else if desc.starts_with("use of unstable library feature 'copy_within") ||
                       desc.starts_with("naming constants with `_` is unstable") ||
                       desc.starts_with("enum variants on type aliases are experimental") {
                        findings.crates.insert((Some(36), name.clone(), ver.clone(), Compat::LikelyIncompatible, desc.into()));
                    }
                    else if desc.starts_with("For more information about an error") ||
                        desc.starts_with("Some errors have detailed explanations") ||
                        desc.starts_with("`#![feature]` may not be used on the stable release") ||
                        desc.starts_with("#![feature] may not be used on the stable release channel") ||
                        desc.starts_with("For more information about this error, try") ||
                        desc.starts_with("cannot continue compilation due to previous err") ||
                        desc.starts_with("Some errors occurred: E0") ||
                        desc.starts_with("aborting due to") {
                        continue;
                    } else if printed.insert(desc.to_string()) {
                        info!("• err: {} ({})", desc, name);
                    }
                }

                if msg.target.as_ref().and_then(|t| t.edition.as_ref()).map_or(false, |e| e == "2018") {
                    findings.crates.insert((Some(30), name.clone(), ver.clone(), Compat::DefinitelyIncompatible, "edition 2018".into()));
                }
                if msg.target.as_ref().and_then(|t| t.edition.as_ref()).map_or(false, |e| e == "2021") {
                    findings.crates.insert((Some(55), name.clone(), ver.clone(), Compat::DefinitelyIncompatible, "edition 2021".into()));
                }
                if level == "error" {
                    findings.crates.insert((None, name, ver, Compat::SuspectedIncompatible, desc.unwrap_or(line).into()));
                } else if reason == "compiler-artifact" {
                    findings.crates.insert((None, name, ver, Compat::VerifiedWorks, "ok".into()));
                } else if level != "warning" && reason != "build-script-executed" && !(level.is_empty() && reason == "compiler-message") {
                    warn!("unknown line {} {} {}", level, reason, line);
                }
            } else if let Some(success) = msg.success {
                let (compat, reason) = if success {
                    (Compat::ProbablyWorks, "bin?".into())
                } else {
                    (Compat::BrokenDeps, findings.crates.iter().filter_map(|(rustc_ver, name, ver, c, reason)| {
                        if c.successful() { return None; }
                        match (findings.rustc_version, *rustc_ver) {
                            (Some(build), Some(other)) if other < build => return None,
                            _ => {},
                        };
                        Some(format!("{name}@{ver}: {reason}"))
                    }).collect::<Vec<_>>().join("\n"))
                };
                findings.crates.insert((None, top_level_crate_name.to_owned(), top_level_crate_ver.to_owned(), compat, reason));
            } else {
                warn!("• Odd compiler message: {}", line);
            }
        } else {
            error!("Does not parse as JSON: {}", line);
        }
    }

    let mut last_broken_manifest_crate: Option<(String, SemVer)> = None;
    for line in stderr.split('\n') {
        if line.trim_start().starts_with("error:") || // there may be multiple errors, not all referring to the last known crate
        line.starts_with("  process didn't exit successfully:") || // handled elsewhere
        line.starts_with("  no targets specified in the manifest") {
            last_broken_manifest_crate = None;

            // error: msg handling
            if line.starts_with("error: failed to select a version for the requirement ") || line.starts_with("error: cyclic package dependency: package `") {
                some_deps_broken = true;
            } else if line.contains("cannot be built because it requires rustc") {
                some_deps_broken = true;
                last_broken_manifest_crate = None;
                let pattern = regex::Regex::new(r"package `([^ ]+) v([^` ]+)` cannot be built because it requires rustc 1.([0-9]+)[^ ]* or newer, while the currently active rustc version is 1.").expect("regex syntax2");
                if let Some(cap) = pattern.captures(line) {
                    if let Ok(ver) = SemVer::parse(&cap[2]) {
                        let rustc_version: u16 = cap[3].parse().expect("cargo rustc ver?");
                        findings.crates.insert((Some(rustc_version - 1), cap[1].to_string(), ver, Compat::DefinitelyIncompatible, line.into()));
                    }
                }
            }
        }
        // manifest parsing handling
        else if let Some(rest) = line.strip_prefix("  failed to parse manifest at `/home/rustyuser/.cargo/registry/src/github.com-1ecc6299db9ec823/") {
            let pattern = regex::Regex::new(r"([^.+/; ]+?)-([0-9]+\.[^/; ]+)/Cargo.toml").expect("regex syntax");
            if let Some(cap) = pattern.captures(rest) {
                if let Ok(ver) = SemVer::parse(&cap[2]) {
                    last_broken_manifest_crate = Some((cap[1].to_string(), ver));
                }
            } else {
                log::error!("bad crate name in path? {}", rest);
            }
        }
        else if line.starts_with("  feature `profile-overrides` is required") {
            if let Some((name, ver)) = last_broken_manifest_crate.take() {
                findings.crates.insert((Some(40), name, ver, Compat::DefinitelyIncompatible, line.into()));
            }
        }
        else if line.starts_with("  invalid type: boolean `true`, expected a string for key `package.readme`") {
            if let Some((name, ver)) = last_broken_manifest_crate.take() {
                findings.crates.insert((Some(45), name, ver, Compat::DefinitelyIncompatible, line.into())); // not sure which version, may be a later one
            }
        }
        else if line.starts_with("  feature `default-run` is required") {
            if let Some((name, ver)) = last_broken_manifest_crate.take() {
                findings.crates.insert((Some(36), name, ver, Compat::DefinitelyIncompatible, line.into()));
            }
        }
        else if line.starts_with("  editions are unstable") || line.starts_with("  feature `rename-dependency` is required") {
            if let Some((name, ver)) = last_broken_manifest_crate.take() {
                findings.crates.insert((Some(30), name, ver, Compat::DefinitelyIncompatible, line.into()));
            }
        }
        else if line.starts_with("  unknown cargo feature `resolver`") || line.starts_with("  feature `resolver` is required") {
            if let Some((name, ver)) = last_broken_manifest_crate.take() {
                findings.crates.insert((Some(50), name, ver, Compat::DefinitelyIncompatible, line.into()));
            }
        }
        else if line.starts_with("  this version of Cargo is older than the `2021` edition") ||
        line.starts_with("  supported edition values are `2015` or `2018`, but `2021` is unknown") ||
        line.starts_with("  feature `edition2021` is required") {
            if let Some((name, ver)) = last_broken_manifest_crate.take() {
                findings.crates.insert((Some(55), name, ver, Compat::DefinitelyIncompatible, line.into()));
            }
        }
        else if let Some(c) = user_time.captures(line) {
            let m: u32 = c[1].parse().expect("time");
            let s: f32 = c[2].parse().expect("time");
            findings.check_time = Some((m * 60) as f32 + s);
        }
    }
    if findings.crates.is_empty() {
        return Err("no info found".into());
    }

    // this is slightly inaccurate, because we don't know if older deps would work
    // but not marking it as failure makes builder retry the crate over and over again
    let has_toplevel_crate_compat = findings.crates.iter().any(|c| c.1 == top_level_crate_name);
    some_deps_broken = some_deps_broken || findings.crates.iter().any(|c| c.0.is_none() && !c.3.successful());
    if !has_toplevel_crate_compat && some_deps_broken {
        let reason = findings.crates.iter().filter_map(|(rustc_ver, name, ver, c, reason)| {
            if c.successful() { return None; }
            match (findings.rustc_version, *rustc_ver) {
                (Some(build), Some(other)) if other < build => return None,
                _ => {},
            };
            Some(format!("{name}@{ver}: {reason}"))
        }).collect::<Vec<_>>().join("\n");
        findings.crates.insert((None, top_level_crate_name.to_owned(), top_level_crate_ver, Compat::BrokenDeps, reason));
    }
    Ok(findings)
}

#[test]
fn parse_cargo() {
    let stderr = r##"
error: failed to download `search-autocompletion v0.3.0`

Caused by:
  unable to get packages from source

Caused by:
  failed to parse manifest at `/home/rustyuser/.cargo/registry/src/github.com-1ecc6299db9ec823/search-autocompletion-0.3.0/Cargo.toml`

Caused by:
  feature `profile-overrides` is required

consider adding `cargo-features = ["profile-overrides"]` to the manifest
error: failed to download `search-autocompletion v0.3.0`

Caused by:
  unable to get packages from source

Caused by:
  failed to parse manifest at `/home/rustyuser/.cargo/registry/src/github.com-1ecc6299db9ec823/search-autocompletion-0.3.0/Cargo.toml`
"##;

    let f = parse_analysis("CHECKING 1.37.0 watever 1.2.3", stderr).unwrap();

    assert_eq!(f.crates.len(), 1);
    let f = f.crates.into_iter().next().unwrap();
    assert_eq!(40, f.0.unwrap());
    assert_eq!("search-autocompletion", f.1);
    assert_eq!("0.3.0", f.2.to_string());
}

#[test]
fn parse_rustc_version_cargo() {
    let stderr = r##"
error: package `fooo v0.1.230` cannot be built because it requires rustc 1.999.2 or newer, while the currently active rustc version is 1.61.0
"##;

    let mut f = parse_analysis("CHECKING 1.61.0 watever 1.2.3", stderr).unwrap();

    f.crates.retain(|(_, name, ..)| name == "fooo");

    assert_eq!(f.crates.len(), 1, "{:?}", f.crates);
    let f = f.crates.into_iter().next().unwrap();
    assert_eq!(998, f.0.unwrap());
    assert_eq!("fooo", f.1);
    assert_eq!("0.1.230", f.2.to_string());
}

#[test]
fn parse_test() {
    let out = r##"

garbage
---XBdt8MziQTMWYwcSsHz1---
CHECKING 1.37.0 wat 6.7.8

{"reason":"compiler-artifact","package_id":"proc_vector2d 1.0.2 (registry+https://github.com/rust-lang/crates.io-index)","target":{"kind":["proc-macro"],"crate_types":["proc-macro"],"name":"proc_vector2d","src_path":"/usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs","edition":"2018","doctest":true},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/tmp/cargo-target-dir/debug/deps/libproc_vector2d-a0e1c737778cdd0d.so"],"executable":null,"fresh":false}
{"reason":"compiler-artifact","package_id":"vector2d 2.2.0 (path+file:///crate)","target":{"kind":["lib"],"crate_types":["lib"],"name":"vector2d","src_path":"/crate/src/lib.rs","edition":"2018","doctest":true},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/tmp/cargo-target-dir/debug/deps/libvector2d-f9ac6cbd40409fbe.rmeta"],"executable":null,"fresh":false}
---XBdt8MziQTMWYwcSsHz1---
CHECKING 1.34.2 wat 6.7.8

{"reason":"compiler-artifact","package_id":"proc_vector2d 1.0.2 (registry+https://github.com/rust-lang/crates.io-index)","target":{"kind":["proc-macro"],"crate_types":["proc-macro"],"name":"proc_vector2d","src_path":"/usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs","edition":"2018"},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/tmp/cargo-target-dir/debug/deps/libproc_vector2d-9470d66afa730e34.so"],"executable":null,"fresh":false}
{"reason":"compiler-artifact","package_id":"vector2d 2.2.0 (path+file:///crate)","target":{"kind":["lib"],"crate_types":["lib"],"name":"vector2d","src_path":"/crate/src/lib.rs","edition":"2018"},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/tmp/cargo-target-dir/debug/deps/libvector2d-59c2022ebc0120a6.rmeta"],"executable":null,"fresh":false}
---XBdt8MziQTMWYwcSsHz1---
CHECKING 1.24.1 toplevelcrate 1.0.1-testcrate

{"message":{"children":[],"code":null,"level":"error","message":"function-like proc macros are currently unstable (see issue #38356)","rendered":"error: function-like proc macros are currently unstable (see issue #38356)\n --> /usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs:4:1\n  |\n4 | #[proc_macro]\n  | ^^^^^^^^^^^^^\n\n","spans":[{"byte_end":68,"byte_start":55,"column_end":14,"column_start":1,"expansion":null,"file_name":"/usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs","is_primary":true,"label":null,"line_end":4,"line_start":4,"suggested_replacement":null,"text":[{"highlight_end":14,"highlight_start":1,"text":"#[proc_macro]"}]}]},"package_id":"proc_vector2d 1.0.2 (registry+https://github.com/rust-lang/crates.io-index)","reason":"compiler-message","target":{"crate_types":["proc-macro"],"kind":["proc-macro"],"name":"proc_vector2d","src_path":"/usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs"}}
{"message":{"children":[],"code":null,"level":"error","message":"function-like proc macros are currently unstable (see issue #38356)","rendered":"error: function-like proc macros are currently unstable (see issue #38356)\n  --> /usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs:18:1\n   |\n18 | #[proc_macro]\n   | ^^^^^^^^^^^^^\n\n","spans":[{"byte_end":360,"byte_start":347,"column_end":14,"column_start":1,"expansion":null,"file_name":"/usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs","is_primary":true,"label":null,"line_end":18,"line_start":18,"suggested_replacement":null,"text":[{"highlight_end":14,"highlight_start":1,"text":"#[proc_macro]"}]}]},"package_id":"proc_vector2d 1.0.2 (registry+https://github.com/rust-lang/crates.io-index)","reason":"compiler-message","target":{"crate_types":["proc-macro"],"kind":["proc-macro"],"name":"proc_vector2d","src_path":"/usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs"}}
{"message":{"children":[],"code":null,"level":"error","message":"aborting due to 2 previous errors","rendered":"error: aborting due to 2 previous errors\n\n","spans":[]},"package_id":"proc_vector2d 1.0.2 (registry+https://github.com/rust-lang/crates.io-index)","reason":"compiler-message","target":{"crate_types":["proc-macro"],"kind":["proc-macro"],"name":"proc_vector2d","src_path":"/usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs"}}
{"reason":"build-finished","success":false}
"##;

    let err = r##"WARNING: Your kernel does not support swap limit capabilities or the cgroup is not mounted. Memory limited without swap.
---XBdt8MziQTMWYwcSsHz1---
+ rustup show
+ cargo check --locked --message-format=json
   Compiling proc_vector2d v1.0.2
    Checking vector2d v2.2.0 (/crate)
    Finished dev [unoptimized + debuginfo] target(s) in 1.39s

real    0m1.413s
user    0m0.880s
sys 0m0.376s
---XBdt8MziQTMWYwcSsHz1---
+ rustup default 1.34.2
info: using existing install for '1.34.2-x86_64-unknown-linux-gnu'
info: default toolchain set to '1.34.2-x86_64-unknown-linux-gnu'
+ cargo check --locked --message-format=json
    Updating `/crate/.cargo/lts-repo-at-c2f8becb5afbc616061cd4e8fffd4a1b50931d3c` index
   Compiling proc_vector2d v1.0.2
    Checking vector2d v2.2.0 (/crate)
    Finished dev [unoptimized + debuginfo] target(s) in 1.63s

real    0m1.660s
user    0m1.060s
sys 0m0.412s
---XBdt8MziQTMWYwcSsHz1---
+ rustup default 1.24.1
info: using existing install for '1.24.1-x86_64-unknown-linux-gnu'
info: default toolchain set to '1.24.1-x86_64-unknown-linux-gnu'
+ cargo check --locked --message-format=json
warning: unused manifest key: package.edition
   Compiling proc_vector2d v1.0.2
error: Could not compile `proc_vector2d`.

Caused by:
  process didn't exit successfully: `rustc --crate-name proc_vector2d /usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs --error-format json --crate-type proc-macro --emit=dep-info,link -C prefer-dynamic -C debuginfo=2 -C metadata=991e439ea4bc3c99 -C extra-filename=-991e439ea4bc3c99 --out-dir /tmp/cargo-target-dir/debug/deps -L dependency=/tmp/cargo-target-dir/debug/deps --cap-lints allow` (exit code: 101)

real    0m0.978s
user    0m0.648s
sys 0m0.180s

exit failure
"##;

    let res = parse_analyses(out, err);
    assert!(res[0].crates.get(&(None, "vector2d".into(), "2.2.0".parse().unwrap(), Compat::VerifiedWorks, "ok".into())).is_some(), "{res:#?}");
    assert!((res[0].check_time.unwrap() - 0.880) < 0.001);
    assert!(res[0].crates.get(&(Some(30), "proc_vector2d".into(), "1.0.2".parse().unwrap(), Compat::DefinitelyIncompatible, "edition 2018".into())).is_some(), "{res:#?}");
    assert!(res[1].crates.get(&(None, "vector2d".into(), "2.2.0".parse().unwrap(), Compat::VerifiedWorks, "ok".into())).is_some(), "{res:#?}");
    assert!(res[1].crates.get(&(Some(30), "proc_vector2d".into(), "1.0.2".parse().unwrap(), Compat::DefinitelyIncompatible, "edition 2018".into())).is_some(), "{res:#?}");
    assert!(res[2].crates.get(&(None, "proc_vector2d".into(), "1.0.2".parse().unwrap(), Compat::SuspectedIncompatible, "function-like proc macros are currently unstable (see issue #38356)".into())).is_some(), "{res:#?}");
    assert!(res[2].crates.get(&(None, "toplevelcrate".into(), "1.0.1-testcrate".parse().unwrap(), Compat::BrokenDeps, "proc_vector2d@1.0.2: function-like proc macros are currently unstable (see issue #38356)".into())).is_some(), "{res:#?}");
}
