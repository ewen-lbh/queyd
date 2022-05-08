prod:
	cross build --target x86_64-unknown-linux-musl --release
	ssh ubuntu@ewen.works 'sudo systemctl stop queyd'
	scp target/x86_64-unknown-linux-musl/release/queyd ubuntu@ewen.works:~/www/notes.ewen.works/
	ssh ubuntu@ewen.works 'sudo systemctl start queyd'
