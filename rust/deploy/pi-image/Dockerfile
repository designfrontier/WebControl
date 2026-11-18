# Roughly Raspberry Pi OS Lite 64-bit, with systemd as PID 1, for update-docker-test.sh.
ARG DEBIAN=trixie
FROM debian:${DEBIAN}

RUN apt-get update && apt-get install -y --no-install-recommends \
      systemd systemd-sysv dbus sudo python3 curl ca-certificates iproute2 procps \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -m -s /bin/bash -G sudo,dialout pi \
    && echo 'pi ALL=(ALL) NOPASSWD: ALL' >/etc/sudoers.d/010_pi-nopasswd \
    && systemctl mask getty@tty1.service systemd-udevd.service

STOPSIGNAL SIGRTMIN+3
CMD ["/lib/systemd/systemd"]
