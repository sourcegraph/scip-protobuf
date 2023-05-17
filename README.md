# scip protobuf

Extra WIP

Only compatible with protobuf 3 files

```
scripts/test.sh
```

```
protoc --plugin=$PWD/target/debug/protoc-gen-scip --scip_out=./test/out ./test/in/hello.proto -Itest/in --scip_opt="$PWD/test/in $PWD/test/out/hello.proto.scip"
./scip snapshot --from test/out/hello.proto.scip
```
