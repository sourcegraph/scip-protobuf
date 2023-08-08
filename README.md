# scip-protobuf

Capable of indexing Protobuf files. Planned to be able to index generated files as well.

Only tested with Protobuf version 3 schemas.

```
scripts/test.sh
```

```
protoc --plugin=$PWD/target/debug/protoc-gen-scip --scip_out=./test/out ./test/in/hello.proto -Itest/in --scip_opt="$PWD/test/in $PWD/test/out/hello.proto.scip"
./scip snapshot --from test/out/hello.proto.scip
```

## License

Apache-2.0
