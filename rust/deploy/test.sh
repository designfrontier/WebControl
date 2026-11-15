#!/usr/bin/env bash
# Tests wc-switch and deploy.sh against stub curl, systemctl, ssh and scp; nothing touches a real system.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/bin"
failures=0

# curl answers with $work/state, or fails when it is missing; systemctl appends its arguments to $work/calls.
cat > "$work/bin/curl" <<EOF
#!/usr/bin/env bash
[ -f "$work/state" ] && cat "$work/state" || exit 7
EOF
cat > "$work/bin/systemctl" <<EOF
#!/usr/bin/env bash
echo "systemctl \$*" >> "$work/calls"
case "\$1" in
  enable) [ "\$3" = webcontrol-rs ] && echo '{"backend":"rust","jobInProgress":false}' > "$work/state" \
          || echo '{"jobInProgress":false}' > "$work/state" ;;
  is-active) echo active ;;
  is-enabled) echo enabled ;;
esac
EOF
chmod +x "$work/bin/"*

run() { PATH="$work/bin:$PATH" WC_SYSTEMCTL=systemctl WC_WAIT=2 "$here/wc-switch" "$@"; }

check() {
  local name="$1"; shift
  if "$@"; then echo "ok   $name"; else echo "FAIL $name"; failures=$((failures + 1)); fi
}

reset() { rm -f "$work/calls" "$work/state"; }

switch_to_rust() {
  reset
  echo '{"backend":"python","jobInProgress":false}' > "$work/state"
  run rust > "$work/out" && grep -q "now the rust backend" "$work/out" &&
    [ "$(cat "$work/calls")" = "$(printf 'systemctl disable --now webcontrol\nsystemctl enable --now webcontrol-rs')" ]
}

switch_to_python_without_a_backend_field() {
  reset
  echo '{"backend": "rust", "jobInProgress": false}' > "$work/state"
  run python > "$work/out" && grep -q "now the python backend" "$work/out" &&
    grep -q "disable --now webcontrol-rs" "$work/calls"
}

refuse_during_a_job() {
  reset
  echo '{"backend":"python","jobInProgress": true}' > "$work/state"
  ! run rust 2> "$work/err" && grep -q "job is running" "$work/err" && [ ! -f "$work/calls" ]
}

force_during_a_job() {
  reset
  echo '{"backend":"python","jobInProgress":true}' > "$work/state"
  run rust --force > /dev/null && grep -q "enable --now webcontrol-rs" "$work/calls"
}

time_out_when_nothing_answers() {
  reset
  cat > "$work/bin/systemctl" <<EOF
#!/usr/bin/env bash
echo "systemctl \$*" >> "$work/calls"
EOF
  ! run rust 2> "$work/err" && grep -q "did not answer as rust" "$work/err"
}

status_lists_both_units() {
  reset
  run status > "$work/out" && grep -q "answering on http://localhost:5000: nothing" "$work/out" &&
    grep -q "^webcontrol-rs" "$work/out"
}

bad_arguments() {
  ! run 2> /dev/null && ! run docker 2> /dev/null
}

check "switches to rust: stops python, starts rust, waits for it" switch_to_rust
check "switches to python, which may lack the backend field" switch_to_python_without_a_backend_field
check "refuses while a job is in progress" refuse_during_a_job
check "--force switches during a job" force_during_a_job
check "fails when the backend never answers" time_out_when_nothing_answers
check "status lists both units" status_lists_both_units
check "refuses unknown arguments" bad_arguments

# deploy.sh push/rollback: ssh and scp stubs record what would run on the Pi.
cat > "$work/bin/ssh" <<EOF
#!/usr/bin/env bash
echo "ssh \$*" >> "$work/calls"
case "\$*" in *api/state*) cat "$work/state" ;; esac
EOF
cat > "$work/bin/scp" <<EOF
#!/usr/bin/env bash
echo "scp \$*" >> "$work/calls"
EOF
chmod +x "$work/bin/"*
touch "$work/webcontrol-rs"
deploy() { PATH="$work/bin:$PATH" WC_BINARY="$work/webcontrol-rs" WC_SKIP_BUILD=1 "$here/../deploy.sh" "$@"; }

install_stages_the_sudoers_rule() {
  reset
  deploy install pi@other.local > /dev/null && grep -q "scp .*webcontrol-rs.sudoers pi@other.local:" "$work/calls"
}

push_to_the_new_card_by_default() {
  reset
  echo '{"jobInProgress":false}' > "$work/state"
  deploy push > /dev/null && grep -q "scp .*webcontrol-rs pi@maslow-next.local:/tmp/webcontrol-rs.new" "$work/calls" &&
    grep -q "try-restart webcontrol-rs" "$work/calls"
}

push_refuses_during_a_job() {
  reset
  echo '{"jobInProgress":true}' > "$work/state"
  ! deploy push pi@other.local 2> "$work/err" && grep -q "job is running" "$work/err" && ! grep -q scp "$work/calls"
}

rollback_swaps_prev_back() {
  reset
  deploy rollback pi@other.local > /dev/null && grep -q "ssh pi@other.local .*webcontrol-rs.prev" "$work/calls"
}

check "install stages the power-off sudoers rule" install_stages_the_sudoers_rule
check "push copies to pi@maslow-next.local and restarts only a running Rust backend" push_to_the_new_card_by_default
check "push refuses while a job is in progress" push_refuses_during_a_job
check "rollback puts the previous binary back" rollback_swaps_prev_back

stage="$work/stage"
mkdir -p "$stage"
cp "$here/webcontrol-rs.service" "$here/webcontrol-conflicts.conf" "$here/wc-switch" "$here/webcontrol-rs.sudoers" "$stage/"
printf '#!/bin/sh\necho "webcontrol-rs test"\n' > "$stage/webcontrol-rs"
install_pi() {
  PATH="$work/bin:$PATH" WC_ROOT="$work/root" WC_SYSTEMCTL=systemctl WC_SUDO=env WC_USER=maker \
    bash "$here/../install-pi.sh" "$stage"
}

installs_beside_python_without_starting() {
  reset
  install_pi > "$work/out" &&
    [ -x "$work/root/opt/webcontrol-rs/webcontrol-rs" ] &&
    grep -q "^User=maker$" "$work/root/etc/systemd/system/webcontrol-rs.service" &&
    grep -q "^Conflicts=webcontrol.service$" "$work/root/etc/systemd/system/webcontrol-rs.service" &&
    grep -q "^Conflicts=webcontrol-rs.service$" "$work/root/etc/systemd/system/webcontrol.service.d/conflicts.conf" &&
    [ -x "$work/root/usr/local/bin/wc-switch" ] &&
    grep -qx "maker ALL=(root) NOPASSWD: /usr/bin/systemctl poweroff" "$work/root/etc/sudoers.d/webcontrol-rs" &&
    [ "$(stat -c %a "$work/root/etc/sudoers.d/webcontrol-rs" 2> /dev/null ||
      stat -f %Lp "$work/root/etc/sudoers.d/webcontrol-rs")" = 440 ] &&
    [ "$(cat "$work/calls")" = "systemctl daemon-reload" ] &&
    grep -q "Python backend is untouched" "$work/out"
}

reinstall_keeps_the_previous_binary() {
  install_pi > /dev/null && [ -f "$work/root/opt/webcontrol-rs/webcontrol-rs.prev" ]
}

check "install-pi.sh installs beside Python and starts nothing" installs_beside_python_without_starting
check "install-pi.sh keeps the previous binary as .prev" reinstall_keeps_the_previous_binary

if [ "$failures" != 0 ]; then
  echo "$failures failed"
  exit 1
fi
echo "all deploy script tests passed"
