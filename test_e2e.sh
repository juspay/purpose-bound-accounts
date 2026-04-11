#!/usr/bin/env bash
# E2E test suite for pba-service
set -euo pipefail

BASE="http://127.0.0.1:3030"
PASS=0
FAIL=0

ok()   { PASS=$((PASS+1)); echo "  PASS: $1"; }
fail() { FAIL=$((FAIL+1)); echo "  FAIL: $1 — $2"; }

# ── 1. List purpose types ────────────────────────────────────
echo "=== Test 1: List purpose types ==="
PURPOSES=$(curl -sf "$BASE/purpose-types")
COUNT=$(echo "$PURPOSES" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))")
[ "$COUNT" -ge 4 ] && ok "Got $COUNT purpose types" || fail "Expected >=4 purpose types" "got $COUNT"

# ── 2. Create health account ────────────────────────────────
echo "=== Test 2: Create health account ==="
ACCT=$(curl -sf -X POST "$BASE/accounts" \
  -H 'Content-Type: application/json' \
  -d '{"holder_id":"aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa","purpose_code":"health","origin_ifsc":"HDFC0001234","origin_account_number":"1234567890"}')
ACCT_ID=$(echo "$ACCT" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")
[ -n "$ACCT_ID" ] && ok "Created account $ACCT_ID" || fail "No account ID" "$ACCT"

# ── 3. Initial balance is zero ──────────────────────────────
echo "=== Test 3: Initial balance ==="
BAL=$(curl -sf "$BASE/accounts/$ACCT_ID/balance")
TOTAL=$(echo "$BAL" | python3 -c "import sys,json; print(json.load(sys.stdin)['total'])")
[ "$TOTAL" = "0" ] && ok "Balance is 0" || fail "Expected 0" "$TOTAL"

# ── 4. Deposit from origin (self-pool) ──────────────────────
echo "=== Test 4: Deposit to self-pool ==="
DEP=$(curl -sf -X POST "$BASE/accounts/$ACCT_ID/deposits" \
  -H 'Content-Type: application/json' \
  -d '{"source_ifsc":"HDFC0001234","source_account_number":"1234567890","amount":10000}')
POOL=$(echo "$DEP" | python3 -c "import sys,json; print(json.load(sys.stdin)['pool'])")
[ "$POOL" = "self_contribution" ] && ok "Deposited to self-pool" || fail "Expected self_contribution" "$POOL"

# ── 5. Deposit from other (others-pool) ─────────────────────
echo "=== Test 5: Deposit to others-pool ==="
DEP2=$(curl -sf -X POST "$BASE/accounts/$ACCT_ID/deposits" \
  -H 'Content-Type: application/json' \
  -d '{"source_ifsc":"ICIC0009999","source_account_number":"9876543210","amount":5000}')
POOL2=$(echo "$DEP2" | python3 -c "import sys,json; print(json.load(sys.stdin)['pool'])")
[ "$POOL2" = "others_contribution" ] && ok "Deposited to others-pool" || fail "Expected others_contribution" "$POOL2"

# ── 6. Verify balances ──────────────────────────────────────
echo "=== Test 6: Verify balances after deposits ==="
BAL2=$(curl -sf "$BASE/accounts/$ACCT_ID/balance")
SELF_BAL=$(echo "$BAL2" | python3 -c "import sys,json; print(json.load(sys.stdin)['self_contribution'])")
OTHERS_BAL=$(echo "$BAL2" | python3 -c "import sys,json; print(json.load(sys.stdin)['others_contribution'])")
TOTAL2=$(echo "$BAL2" | python3 -c "import sys,json; print(json.load(sys.stdin)['total'])")
[ "$SELF_BAL" = "10000" ] && [ "$OTHERS_BAL" = "5000" ] && [ "$TOTAL2" = "15000" ] \
  && ok "Balances correct: self=$SELF_BAL, others=$OTHERS_BAL, total=$TOTAL2" \
  || fail "Wrong balances" "self=$SELF_BAL, others=$OTHERS_BAL, total=$TOTAL2"

# ── 7. Payment from others-pool only ────────────────────────
echo "=== Test 7: Payment (others-pool only, amount <= others balance) ==="
PAY=$(curl -sf -X POST "$BASE/accounts/$ACCT_ID/payments" \
  -H 'Content-Type: application/json' \
  -d '{"amount":3000,"merchant_mcc":"5912","merchant_id":"PHARMACY001","description":"test pharmacy"}')
FROM_OTHERS=$(echo "$PAY" | python3 -c "import sys,json; print(json.load(sys.stdin)['from_others'])")
FROM_SELF=$(echo "$PAY" | python3 -c "import sys,json; print(json.load(sys.stdin)['from_self'])")
[ "$FROM_OTHERS" = "3000" ] && [ "$FROM_SELF" = "0" ] \
  && ok "Payment from others-pool: others=$FROM_OTHERS, self=$FROM_SELF" \
  || fail "Wrong split" "others=$FROM_OTHERS, self=$FROM_SELF"

# ── 8. Payment split (others insufficient, uses both pools) ─
echo "=== Test 8: Payment (split across both pools) ==="
PAY2=$(curl -sf -X POST "$BASE/accounts/$ACCT_ID/payments" \
  -H 'Content-Type: application/json' \
  -d '{"amount":4000,"merchant_mcc":"8011","merchant_id":"DOCTOR001","description":"test doctor"}')
FROM_OTHERS2=$(echo "$PAY2" | python3 -c "import sys,json; print(json.load(sys.stdin)['from_others'])")
FROM_SELF2=$(echo "$PAY2" | python3 -c "import sys,json; print(json.load(sys.stdin)['from_self'])")
# others had 2000 left (5000-3000), so should use all 2000 from others + 2000 from self
[ "$FROM_OTHERS2" = "2000" ] && [ "$FROM_SELF2" = "2000" ] \
  && ok "Split payment: others=$FROM_OTHERS2, self=$FROM_SELF2" \
  || fail "Wrong split" "others=$FROM_OTHERS2, self=$FROM_SELF2"

# ── 9. Payment from self-pool only ──────────────────────────
echo "=== Test 9: Payment (self-pool only, others depleted) ==="
PAY3=$(curl -sf -X POST "$BASE/accounts/$ACCT_ID/payments" \
  -H 'Content-Type: application/json' \
  -d '{"amount":1000,"merchant_mcc":"5912","merchant_id":"PHARMACY002","description":"test pharmacy 2"}')
FROM_OTHERS3=$(echo "$PAY3" | python3 -c "import sys,json; print(json.load(sys.stdin)['from_others'])")
FROM_SELF3=$(echo "$PAY3" | python3 -c "import sys,json; print(json.load(sys.stdin)['from_self'])")
[ "$FROM_OTHERS3" = "0" ] && [ "$FROM_SELF3" = "1000" ] \
  && ok "Self-only payment: others=$FROM_OTHERS3, self=$FROM_SELF3" \
  || fail "Wrong split" "others=$FROM_OTHERS3, self=$FROM_SELF3"

# ── 10. Payment insufficient funds ──────────────────────────
echo "=== Test 10: Payment insufficient funds ==="
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE/accounts/$ACCT_ID/payments" \
  -H 'Content-Type: application/json' \
  -d '{"amount":999999,"merchant_mcc":"5912","merchant_id":"PHARMACY003","description":"too much"}')
[ "$HTTP_CODE" = "422" ] && ok "Insufficient funds rejected (HTTP $HTTP_CODE)" || fail "Expected 422" "got $HTTP_CODE"

# ── 11. Payment invalid MCC ─────────────────────────────────
echo "=== Test 11: Payment with invalid MCC for health account ==="
HTTP_CODE2=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE/accounts/$ACCT_ID/payments" \
  -H 'Content-Type: application/json' \
  -d '{"amount":100,"merchant_mcc":"4011","merchant_id":"RAILWAY001","description":"train ticket"}')
[ "$HTTP_CODE2" = "422" ] && ok "Invalid MCC rejected (HTTP $HTTP_CODE2)" || fail "Expected 422" "got $HTTP_CODE2"

# ── 12. Withdrawal from self-pool ────────────────────────────
echo "=== Test 12: Withdrawal from self-pool ==="
WD=$(curl -sf -X POST "$BASE/accounts/$ACCT_ID/withdrawals" \
  -H 'Content-Type: application/json' \
  -d '{"amount":2000}')
WD_AMT=$(echo "$WD" | python3 -c "import sys,json; print(json.load(sys.stdin)['amount'])")
[ "$WD_AMT" = "2000" ] && ok "Withdrew 2000" || fail "Expected 2000" "$WD_AMT"

# ── 13. Withdrawal insufficient funds ────────────────────────
echo "=== Test 13: Withdrawal exceeding self-pool ==="
HTTP_CODE3=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE/accounts/$ACCT_ID/withdrawals" \
  -H 'Content-Type: application/json' \
  -d '{"amount":999999}')
[ "$HTTP_CODE3" = "422" ] && ok "Insufficient withdrawal rejected (HTTP $HTTP_CODE3)" || fail "Expected 422" "got $HTTP_CODE3"

# ── 14. Get account ─────────────────────────────────────────
echo "=== Test 14: Get account ==="
ACCT_GET=$(curl -sf "$BASE/accounts/$ACCT_ID")
PURPOSE=$(echo "$ACCT_GET" | python3 -c "import sys,json; print(json.load(sys.stdin)['purpose_code'])")
[ "$PURPOSE" = "health" ] && ok "Account purpose is health" || fail "Expected health" "$PURPOSE"

# ── 15. Freeze account ──────────────────────────────────────
echo "=== Test 15: Freeze account ==="
curl -sf -X PATCH "$BASE/accounts/$ACCT_ID/status" \
  -H 'Content-Type: application/json' \
  -d '{"status":"frozen"}' > /dev/null
ACCT_FROZEN=$(curl -sf "$BASE/accounts/$ACCT_ID")
STATUS=$(echo "$ACCT_FROZEN" | python3 -c "import sys,json; print(json.load(sys.stdin)['status'])")
[ "$STATUS" = "frozen" ] && ok "Account frozen" || fail "Expected frozen" "$STATUS"

# ── 16. Deposit on frozen account ────────────────────────────
echo "=== Test 16: Deposit on frozen account ==="
HTTP_CODE4=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE/accounts/$ACCT_ID/deposits" \
  -H 'Content-Type: application/json' \
  -d '{"source_ifsc":"HDFC0001234","source_account_number":"1234567890","amount":100}')
[ "$HTTP_CODE4" = "409" ] && ok "Deposit on frozen account rejected (HTTP $HTTP_CODE4)" || fail "Expected 409" "got $HTTP_CODE4"

# ── 17. Reactivate account ──────────────────────────────────
echo "=== Test 17: Reactivate account ==="
curl -sf -X PATCH "$BASE/accounts/$ACCT_ID/status" \
  -H 'Content-Type: application/json' \
  -d '{"status":"active"}' > /dev/null
ACCT_ACTIVE=$(curl -sf "$BASE/accounts/$ACCT_ID")
STATUS2=$(echo "$ACCT_ACTIVE" | python3 -c "import sys,json; print(json.load(sys.stdin)['status'])")
[ "$STATUS2" = "active" ] && ok "Account reactivated" || fail "Expected active" "$STATUS2"

# ── 18. Duplicate account detection ──────────────────────────
echo "=== Test 18: Duplicate account ==="
HTTP_CODE5=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE/accounts" \
  -H 'Content-Type: application/json' \
  -d '{"holder_id":"aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa","purpose_code":"health","origin_ifsc":"HDFC0001234","origin_account_number":"1234567890"}')
[ "$HTTP_CODE5" = "409" ] && ok "Duplicate rejected (HTTP $HTTP_CODE5)" || fail "Expected 409" "got $HTTP_CODE5"

# ── 19. Get specific purpose type ────────────────────────────
echo "=== Test 19: Get purpose type ==="
PT=$(curl -sf "$BASE/purpose-types/health")
PT_CODE=$(echo "$PT" | python3 -c "import sys,json; print(json.load(sys.stdin)['purpose_code'])")
[ "$PT_CODE" = "health" ] && ok "Got health purpose type" || fail "Expected health" "$PT_CODE"

# ── 20. Cross-purpose MCC rejection ─────────────────────────
echo "=== Test 20: Cross-purpose MCC rejection ==="
# Health account trying to pay at an education MCC (5942 = bookstores)
HTTP_CODE6=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE/accounts/$ACCT_ID/payments" \
  -H 'Content-Type: application/json' \
  -d '{"amount":100,"merchant_mcc":"5942","merchant_id":"BOOKSTORE001","description":"bookstore purchase"}')
[ "$HTTP_CODE6" = "422" ] && ok "Cross-purpose MCC rejected (HTTP $HTTP_CODE6)" || fail "Expected 422" "got $HTTP_CODE6"

# ── 21. Education account with valid education MCC ───────────
echo "=== Test 21: Education account with education MCC ==="
EDU=$(curl -sf -X POST "$BASE/accounts" \
  -H 'Content-Type: application/json' \
  -d '{"holder_id":"bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb","purpose_code":"education","origin_ifsc":"SBIN0005678","origin_account_number":"5678901234"}')
EDU_ID=$(echo "$EDU" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")
# Deposit to education account
curl -sf -X POST "$BASE/accounts/$EDU_ID/deposits" \
  -H 'Content-Type: application/json' \
  -d '{"source_ifsc":"SBIN0005678","source_account_number":"5678901234","amount":5000}' > /dev/null
# Pay at bookstore (education MCC)
EDU_PAY=$(curl -sf -X POST "$BASE/accounts/$EDU_ID/payments" \
  -H 'Content-Type: application/json' \
  -d '{"amount":1000,"merchant_mcc":"5942","merchant_id":"BOOKSTORE001","description":"textbooks"}')
EDU_AMT=$(echo "$EDU_PAY" | python3 -c "import sys,json; print(json.load(sys.stdin)['amount'])")
[ "$EDU_AMT" = "1000" ] && ok "Education payment at bookstore succeeded" || fail "Expected 1000" "$EDU_AMT"

# ── Summary ──────────────────────────────────────────────────
echo ""
echo "================================="
echo "Results: $PASS passed, $FAIL failed out of $((PASS+FAIL)) tests"
echo "================================="
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
