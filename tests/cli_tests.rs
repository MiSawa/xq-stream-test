#[test]
fn cli_test() {
    trycmd::TestCases::new()
        .default_bin_name("stream")
        .case("tests/cmd/*.toml");
}
