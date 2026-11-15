# The new UI is static files, so it builds once on the build machine's own platform, not under ARM emulation.
FROM --platform=$BUILDPLATFORM node:24-bookworm-slim AS frontend
WORKDIR /src/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
COPY contract/ /src/contract/
RUN npm run build

# Bullseye's python3 is 3.9, the version requirements.txt is pinned for.
FROM debian:bullseye-slim

# Install dependencies needed for building and running OpenCV
RUN apt-get update && apt-get install -y --no-install-recommends \
    python3-pip python3-setuptools python3-dev \
    # to build and install
    unzip wget sed \
    build-essential cmake pkg-config \
    libv4l-dev \
    # for opencv math operations
    libatlas-base-dev gfortran \
    # for gevent
    libevent-dev \
    avrdude libffi-dev libxml2-dev libxslt-dev \
    libsm6 libxext6 libxrender-dev git \
    # Install requirements that don't play well with armv7+rpi via pip
    python3-opencv python3-scipy python3-numpy \
    # cleanup
    && apt-get -y autoremove \
    && apt-get clean && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*

# Get other python dependencies
ADD requirements.txt /requirements.txt
# Remove opencv, scipy and numpy from requirements (since they're already installed)
RUN sed -i '/opencv-python.*/d' /requirements.txt && sed -i '/scipy.*/d' /requirements.txt && sed -i '/numpy.*/d' /requirements.txt
# piwheels serves prebuilt armv6/armv7 wheels; other platforms find nothing there and use PyPI.
RUN pip3 install --extra-index-url https://www.piwheels.org/simple -r /requirements.txt

# Build the bundled firmware into /WebControl/firmware/<variant>/<variant>-<version>.hex
RUN apt-get update \
    && pip3 install -U platformio \
    && pio platform install --with-package framework-arduino atmelavr

ADD buildFirmware.sh /buildFirmware.sh
RUN FIRMWARE_DIR=/WebControl/firmware BUILD_DIR=/tmp/firmware-build bash /buildFirmware.sh \
    && rm -rf /tmp/firmware-build

ADD . /WebControl
COPY --from=frontend /src/frontend/dist /WebControl/frontend/dist

# Pre-compile the pyc files (for faster Docker image startup)
RUN python3 -m compileall /WebControl

WORKDIR /WebControl
EXPOSE 5000/tcp
CMD python3 /WebControl/main.py
