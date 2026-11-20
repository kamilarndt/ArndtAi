.PHONY: verify test demo ci

# Dowody do GATES.md: uruchamia komendy i odswieza checks/evidence.json + craft.json.
verify:
	python3 scripts/verify.py
	python3 scripts/verify.py --assert
	@echo "VERIFY OK"

# To samo, co hosted CI (GitHub Actions nie ma minut: billing) — bramka lokalna.
ci: verify

test:
	cd app && cargo test

demo:
	cd app && cargo run --quiet -- demo
