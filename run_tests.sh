#!/bin/bash
set -e

# Helper: check expected substring in output
assert_contains() {
  local output="$1"
  local expected="$2"
  if [[ "$output" != *"$expected"* ]]; then
    echo "❌ Test failed: expected '$expected' in output:"
    echo "$output"
    exit 1
  else
    echo "✅ Passed: found '$expected'"
  fi
}

echo "🚀 Rebuilding and deploying canister..."
dfx deploy defi_pool_backend

echo ""
echo "=== Alice tests ==="
out=$(dfx canister call defi_pool_backend deposit '("alice", 100:nat)')
assert_contains "$out" "(true)"

out=$(dfx canister call defi_pool_backend deposit_collateral '("alice", 200:nat)')
assert_contains "$out" "(true)"

out=$(dfx canister call defi_pool_backend borrow '("alice", record { amount = 100:nat })')
assert_contains "$out" "(true)"

out=$(dfx canister call defi_pool_backend get_user_account '("alice")')
assert_contains "$out" "borrowed = 100"

echo "Alice repays..."
dfx canister call defi_pool_backend repay '("alice", 50:nat)' >/dev/null
dfx canister call defi_pool_backend repay '("alice", 50:nat)' >/dev/null
out=$(dfx canister call defi_pool_backend get_user_account '("alice")')
assert_contains "$out" "borrowed = 0"

echo "Alice tries to over-borrow..."
out=$(dfx canister call defi_pool_backend borrow '("alice", record { amount = 200:nat })')
assert_contains "$out" "(false)"

echo "Alice borrows 50 then tries to withdraw too much collateral..."
dfx canister call defi_pool_backend borrow '("alice", record { amount = 50:nat })' >/dev/null
out=$(dfx canister call defi_pool_backend withdraw_collateral '("alice", 200:nat)')
assert_contains "$out" "(false)"

echo ""
echo "=== Bob tests ==="
out=$(dfx canister call defi_pool_backend deposit '("bob", 300:nat)')
assert_contains "$out" "(true)"

out=$(dfx canister call defi_pool_backend deposit_collateral '("bob", 500:nat)')
assert_contains "$out" "(true)"

out=$(dfx canister call defi_pool_backend borrow '("bob", record { amount = 200:nat })')
assert_contains "$out" "(true)"

out=$(dfx canister call defi_pool_backend get_user_account '("bob")')
assert_contains "$out" "borrowed = 200"

echo ""
echo "=== Carol tests ==="
echo "Carol tries to borrow without collateral..."
out=$(dfx canister call defi_pool_backend borrow '("carol", record { amount = 100:nat })')
assert_contains "$out" "(false)"

echo "Carol deposits 50..."
dfx canister call defi_pool_backend deposit '("carol", 50:nat)' >/dev/null
out=$(dfx canister call defi_pool_backend get_user_account '("carol")')
assert_contains "$out" "deposited = 50"

echo ""
echo "=== Repayment scenario ==="
echo "Alice repays..."
dfx canister call defi_pool_backend repay '("alice", 50:nat)' >/dev/null
out=$(dfx canister call defi_pool_backend get_user_account '("alice")')
assert_contains "$out" "borrowed = 0"

echo "Bob repays..."
dfx canister call defi_pool_backend repay '("bob", 200:nat)' >/dev/null
out=$(dfx canister call defi_pool_backend get_user_account '("bob")')
assert_contains "$out" "borrowed = 0"

echo "Carol has nothing borrowed, skipping repayment."

echo ""
echo "=== Final StableToken State ==="
out=$(dfx canister call defi_pool_backend get_stable_token)
echo "$out"

# Check balances reflect deposits only (100 + 300 + 50 = 450 supply)
assert_contains "$out" "total_supply = 450"
assert_contains "$out" "record { key = \"alice\"; value = 100"
assert_contains "$out" "record { key = \"bob\"; value = 300"
assert_contains "$out" "record { key = \"carol\"; value = 50"

echo ""
echo "🎉 All tests passed successfully!"

