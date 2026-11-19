.PHONY: verify test demo

# Dowody do GATES.md: uruchamia komendy i odswieza checks/evidence.json + craft.json.
verify:
	python3 scripts/verify.py
	python3 scripts/verify.py --assert
	@echo "VERIFY OK"

test:
	cd app && cargo test

demo:
	cd app && cargo run --quiet -- demo
