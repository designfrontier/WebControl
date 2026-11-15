# Roughly Raspberry Pi OS Lite 64-bit as tools/install_from_source.sh sees it, with systemd as PID 1.
ARG DEBIAN=bookworm
FROM debian:${DEBIAN}

RUN apt-get update && apt-get install -y --no-install-recommends \
      systemd systemd-sysv dbus sudo python3 curl ca-certificates rsync git iproute2 procps \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -m -s /bin/bash -G sudo,dialout pi \
    && echo 'pi ALL=(ALL) NOPASSWD: ALL' >/etc/sudoers.d/010_pi-nopasswd \
    && systemctl mask getty@tty1.service systemd-udevd.service

STOPSIGNAL SIGRTMIN+3
CMD ["/lib/systemd/systemd"]
