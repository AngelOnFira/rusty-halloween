#!/bin/bash

protoc --rust_out ./src/proto_schema/ schema.proto
