#!/bin/bash

protoc --rust_out ./src/proto_schema/ schema.proto
protoc --python_out ./python_transfer/proto_schema schema.proto