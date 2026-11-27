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
  is-active) if grep -qx "\${!#}" "$work/active" 2> /dev/null; then echo active; else echo inactive; exit 3; fi ;;
  cat) if [ -f "$work/no-\$2" ]; then exit 1; fi ;;
  is-enabled) if grep -qx "\${!#}" "$work/enabled" 2> /dev/null; then echo enabled; else echo disabled; exit 1; fi ;;
esac
EOF
chmod +x "$work/bin/"*

run() { PATH="$work/bin:$PATH" WC_SYSTEMCTL=systemctl WC_WAIT=2 "$here/wc-switch" "$@"; }

check() {
  local name="$1"; shift
  if "$@"; then echo "ok   $name"; else echo "FAIL $name"; failures=$((failures + 1)); fi
}

reset() { rm -f "$work/calls" "$work/state" "$work/active" "$work/enabled" "$work"/no-*; }

switch_to_rust() {
  reset
  echo '{"backend":"python","jobInProgress":false}' > "$work/state"
  run rust > "$work/out" && grep -q "now the rust backend" "$work/out" &&
    [ "$(grep -v ' cat ' "$work/calls")" = "$(printf 'systemctl disable --now webcontrol\nsystemctl enable --now webcontrol-rs')" ]
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
  ! run rust 2> "$work/err" && grep -q "job is running" "$work/err" && ! grep -q able "$work/calls"
}

refuse_a_backend_without_a_unit() {
  reset
  touch "$work/no-webcontrol"
  echo '{"backend":"rust","jobInProgress":false}' > "$work/state"
  ! run python 2> "$work/err" && grep -q "no webcontrol.service" "$work/err" && ! grep -q able "$work/calls"
}

force_during_a_job() {
  reset
  echo '{"backend":"python","jobInProgress":true}' > "$work/state"
  run rust --force > /dev/null && grep -q "enable --now webcontrol-rs" "$work/calls"
}

time_out_when_nothing_answers() {
  reset
  cp "$work/bin/systemctl" "$work/systemctl.saved"
  cat > "$work/bin/systemctl" <<EOF
#!/usr/bin/env bash
echo "systemctl \$*" >> "$work/calls"
EOF
  local result=0
  { ! run rust 2> "$work/err" && grep -q "did not answer as rust" "$work/err"; } || result=1
  mv "$work/systemctl.saved" "$work/bin/systemctl"
  return "$result"
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
check "refuses to switch to a backend this disk does not have" refuse_a_backend_without_a_unit
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
  deploy install pi@other.local > /dev/null && grep -q "scp .*webcontrol-rs.sudoers .*pi@other.local:" "$work/calls" &&
    grep -q "scp .*webcontrol-update@.service .*pi@other.local:" "$work/calls"
}

install_stages_the_updater() {
  reset
  deploy install pi@other.local > /dev/null && grep -q "scp .*/webcontrol-update .*pi@other.local:" "$work/calls"
}

push_to_maslow_local_by_default() {
  reset
  echo '{"jobInProgress":false}' > "$work/state"
  deploy push > /dev/null && grep -q "scp .*webcontrol-rs pi@maslow.local:/tmp/webcontrol-rs.new" "$work/calls" &&
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

check "install stages the sudoers rule and the update unit" install_stages_the_sudoers_rule
check "install stages webcontrol-update" install_stages_the_updater
check "push copies to pi@maslow.local and restarts only a running Rust backend" push_to_maslow_local_by_default
check "push refuses while a job is in progress" push_refuses_during_a_job
check "rollback puts the previous binary back" rollback_swaps_prev_back

stage="$work/stage"
mkdir -p "$stage"
cp "$here/webcontrol-rs.service" "$here/webcontrol-conflicts.conf" "$here/wc-switch" "$here/webcontrol-rs.sudoers" \
  "$here/webcontrol-update@.service" "$here/../webcontrol-update" "$stage/"
printf '#!/bin/sh\necho "webcontrol-rs test"\n' > "$stage/webcontrol-rs"
printf '#!/bin/sh\necho "apt $*" >> "%s/apt"\n' "$work" > "$work/bin/apt"
printf '#!/bin/sh\n[ ! -f "%s/visudo-rejects" ]\n' "$work" > "$work/bin/visudo"
printf '#!/bin/sh\necho "$*" >> "%s/sudo"\nexec "$@"\n' "$work" > "$work/bin/sudo-log"
cat > "$work/bin/uname" <<EOF
#!/usr/bin/env bash
if [ "\$1" = -m ]; then cat "$work/arch" 2> /dev/null || echo aarch64; else exec /usr/bin/uname "\$@"; fi
EOF
chmod +x "$work/bin/apt" "$work/bin/visudo" "$work/bin/sudo-log" "$work/bin/uname"
install_pi() {
  PATH="$work/bin:$PATH" WC_ROOT="$work/root" WC_SYSTEMCTL=systemctl WC_SUDO=sudo-log WC_USER=maker WC_APT=apt \
    bash "$here/../install-pi.sh" "$stage"
}

installs_avrdude_unless_present() {
  rm -f "$work/apt"
  install_pi > /dev/null && { grep -q "^apt" "$work/apt" || command -v avrdude > /dev/null; }
}

installs_and_starts_when_nothing_runs() {
  reset
  install_pi > "$work/out" &&
    [ -x "$work/root/opt/webcontrol-rs/webcontrol-rs" ] &&
    grep -q "^User=maker$" "$work/root/etc/systemd/system/webcontrol-rs.service" &&
    grep -q "^Conflicts=webcontrol.service$" "$work/root/etc/systemd/system/webcontrol-rs.service" &&
    grep -q "^Conflicts=webcontrol-rs.service$" "$work/root/etc/systemd/system/webcontrol.service.d/conflicts.conf" &&
    [ -x "$work/root/usr/local/bin/wc-switch" ] &&
    cmp -s "$work/root/usr/local/bin/webcontrol-update" "$here/../webcontrol-update" &&
    [ -x "$work/root/usr/local/bin/webcontrol-update" ] &&
    grep -qx "maker ALL=(root) NOPASSWD: /usr/bin/systemctl poweroff" "$work/root/etc/sudoers.d/webcontrol-rs" &&
    cmp -s "$work/root/etc/systemd/system/webcontrol-update@.service" "$here/webcontrol-update@.service" &&
    [ "$(stat -c %a "$work/root/etc/sudoers.d/webcontrol-rs" 2> /dev/null ||
      stat -f %Lp "$work/root/etc/sudoers.d/webcontrol-rs")" = 440 ] &&
    [ "$(tail -1 "$work/calls")" = "systemctl enable --now webcontrol-rs" ] &&
    grep -q "webcontrol-rs is running and starts at boot" "$work/out"
}

leaves_an_older_webcontrol_running() {
  reset
  echo webcontrol > "$work/active"
  install_pi > "$work/out" && ! grep -q "enable" "$work/calls" && grep -q "wc-switch rust" "$work/out"
}

an_update_waits_for_a_restart() {
  reset
  echo webcontrol-rs > "$work/active"
  install_pi > "$work/out" && [ "$(tail -1 "$work/calls")" = "systemctl enable webcontrol-rs" ] &&
    grep -q "sudo systemctl restart webcontrol-rs" "$work/out"
}

disables_a_stopped_older_webcontrol() {
  reset
  echo webcontrol > "$work/enabled"
  install_pi > /dev/null &&
    [ "$(grep -v is- "$work/calls" | tail -2)" = "$(printf 'systemctl disable webcontrol\nsystemctl enable --now webcontrol-rs')" ]
}

checks_the_sudoers_rule_through_sudo() {
  reset
  rm -f "$work/sudo"
  install_pi > /dev/null && grep -q "^visudo -cqf " "$work/sudo"
}

refuses_a_sudoers_rule_visudo_rejects() {
  reset
  rm -rf "$work/root"
  touch "$work/visudo-rejects"
  ! install_pi > /dev/null 2>&1 && [ ! -e "$work/root/etc/sudoers.d/webcontrol-rs" ]
  local refused=$?
  rm -f "$work/visudo-rejects"
  return "$refused"
}

reinstall_keeps_the_previous_binary() {
  install_pi > /dev/null && [ -f "$work/root/opt/webcontrol-rs/webcontrol-rs.prev" ]
}

# The rule allows exactly these, each with every argument, so nothing user-supplied can reach systemctl.
allows_exactly_poweroff_update_and_rollback() {
  reset
  install_pi > /dev/null &&
    [ "$(grep -v '^#' "$work/root/etc/sudoers.d/webcontrol-rs")" = "$(printf '%s\n' \
      "maker ALL=(root) NOPASSWD: /usr/bin/systemctl poweroff" \
      "maker ALL=(root) NOPASSWD: /usr/bin/systemctl start --no-block webcontrol-update@update.service" \
      "maker ALL=(root) NOPASSWD: /usr/bin/systemctl start --no-block webcontrol-update@rollback.service")" ]
}

the_sudoers_rule_parses() {
  local visudo
  visudo="$(PATH="/usr/sbin:/sbin:$PATH" command -v visudo)" || return 0
  [ "$visudo" != "$work/bin/visudo" ] || return 0
  "$visudo" -cqf "$here/webcontrol-rs.sudoers"
}

the_update_unit_runs_the_updater_as_a_oneshot() {
  local unit="$here/webcontrol-update@.service"
  grep -qx "Type=oneshot" "$unit" && grep -qx "ExecStart=/usr/local/bin/webcontrol-update --service %i" "$unit" &&
    grep -qx "StateDirectory=webcontrol-rs" "$unit" && ! grep -q "^User=" "$unit"
}

# As root, as webcontrol-update@update.service runs it: id -u says 0 and nothing goes through sudo.
installs_as_root_without_sudo() {
  reset
  rm -f "$work/sudo"
  cat > "$work/bin/id" <<'EOF'
#!/bin/sh
if [ "$1" = -u ]; then echo 0; else exec /usr/bin/id "$@"; fi
EOF
  chmod +x "$work/bin/id"
  local result=0
  PATH="$work/bin:$PATH" WC_ROOT="$work/root" WC_USER=maker WC_APT=apt bash "$here/../install-pi.sh" "$stage" \
    > /dev/null || result=1
  rm -f "$work/bin/id"
  [ "$result" = 0 ] && [ ! -e "$work/sudo" ] && grep -q "^systemctl daemon-reload" "$work/calls" &&
    [ -f "$work/root/etc/systemd/system/webcontrol-update@.service" ]
}

on_a_laptop() {
  echo x86_64 > "$work/arch"
  local result=0
  "$@" || result=1
  rm -f "$work/arch"
  return "$result"
}

installs_without_power_off() {
  reset
  rm -rf "$work/root"
  install_pi > "$work/out" &&
    grep -qx "SupplementaryGroups=dialout" "$work/root/etc/systemd/system/webcontrol-rs.service" &&
    [ "$(grep -v '^#' "$work/root/etc/sudoers.d/webcontrol-rs")" = "$(printf '%s\n' \
      "maker ALL=(root) NOPASSWD: /usr/bin/systemctl start --no-block webcontrol-update@update.service" \
      "maker ALL=(root) NOPASSWD: /usr/bin/systemctl start --no-block webcontrol-update@rollback.service")" ] &&
    grep -q "starts at boot: http://localhost:5000/" "$work/out"
}

refuses_another_architecture() {
  reset
  rm -rf "$work/root"
  echo armv7l > "$work/arch"
  ! install_pi > /dev/null 2> "$work/err" && grep -q "aarch64 (the Pi) and x86_64 Linux; this is armv7l" "$work/err" &&
    [ ! -e "$work/root" ]
  local refused=$?
  rm -f "$work/arch"
  return "$refused"
}

check "install-pi.sh installs, enables and starts webcontrol-rs" installs_and_starts_when_nothing_runs
check "install-pi.sh on x86_64 leaves power-off out of the sudoers rule" on_a_laptop installs_without_power_off
check "install-pi.sh refuses anything but aarch64 and x86_64" refuses_another_architecture
check "the sudoers rule allows power-off, update and rollback, exactly" allows_exactly_poweroff_update_and_rollback
check "the sudoers rule passes the system's visudo -c" the_sudoers_rule_parses
check "webcontrol-update@.service runs the updater as root, once" the_update_unit_runs_the_updater_as_a_oneshot
check "install-pi.sh as root calls nothing through sudo" installs_as_root_without_sudo
check "install-pi.sh leaves a running older WebControl alone" leaves_an_older_webcontrol_running
check "install-pi.sh does not restart a running webcontrol-rs" an_update_waits_for_a_restart
check "install-pi.sh disables a stopped older WebControl so it cannot take the port at boot" \
  disables_a_stopped_older_webcontrol
check "install-pi.sh checks the sudoers rule with visudo through sudo" checks_the_sudoers_rule_through_sudo
check "install-pi.sh refuses a sudoers rule visudo rejects" refuses_a_sudoers_rule_visudo_rejects
check "install-pi.sh keeps the previous binary as .prev" reinstall_keeps_the_previous_binary
check "install-pi.sh installs avrdude for the firmware upgrade" installs_avrdude_unless_present

if [ "$failures" != 0 ]; then
  echo "$failures failed"
  exit 1
fi
echo "all deploy script tests passed"
