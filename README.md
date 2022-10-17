# openmicc-server

# Development Notes

Live update when files are changed
```
# check
bacon

# build
cargo watch -x 'build --bin openmicc-server' -s 'touch .trigger'

# run
cargo watch --no-gitignore -w .trigger -x 'run --bin openmicc-server'
```
