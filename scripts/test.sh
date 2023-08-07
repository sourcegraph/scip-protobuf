cargo build

for subdir in test/in/*/ ; do
    test_name=$(basename $subdir)
    echo "Testing $test_name..."
    mkdir -p test/out/$test_name
    protoc $subdir/*.proto -Itest/in/$test_name --descriptor_set_out=test/out/$test_name.dset --include_source_info --include_imports
    ./target/debug/scip-protobuf --root test/in/$test_name --in test/out/$test_name.dset --out test/out/$test_name.scip
    ./scip snapshot --from test/out/$test_name.scip --to test/out/$test_name
done
