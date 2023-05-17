cargo build

for subdir in test/in/*/ ; do
    test_name=$(basename $subdir)
    echo "Testing $test_name..."
    mkdir -p test/out/$test_name
    protoc --plugin=$PWD/target/debug/protoc-gen-scip --scip_out=./test/out/$test_name $subdir/*.proto -Itest/in/$test_name --scip_opt="$PWD/test/in/$test_name $PWD/test/out/$test_name.scip"
    ./scip snapshot --from test/out/$test_name.scip --to test/out/$test_name
done
