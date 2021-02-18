# P2P Gossip

```
cargo build
```

Then in one terminal:
```
./target/debug/p2p-gossip --period 1 --port 5000
```

and in another one:
```
./target/debug/p2p-gossip --period 1 --port 5001 --connect 127.0.0.1:5000
```

