FROM debian:buster-slim

# Install dependencies needed for building and running OpenCV
RUN apt-get update && apt-get install -y --no-install-recommends \
    python3-pip python3-setuptools python3-dev \
    # to build and install
    unzip wget sed \
    build-essential cmake pkg-config \
    # to work with images (likely not necessary for optical calibration)
    # libjpeg-dev libtiff-dev libpng-dev \
    # to work with video / webcam (likely not necessary for optical calibration)
    # libavcodec-dev libavformat-dev libswscale-dev \
    libv4l-dev \
    # for opencv math operations
    libatlas-base-dev gfortran \
    # thread building blocks (not availabe on the arm version of python:3.5.6-slim-stretch)
    # libtbb2 libtbb-dev \
    # for gevent
    libevent-dev \
    # for numpy (installs libf77blas.so.3)
    libatlas-base-dev \
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
RUN pip3 install -r /requirements.txt

# Build the bundled firmware into /WebControl/firmware/<variant>/<variant>-<version>.hex
RUN apt-get update \
    && pip3 install -U platformio \
    && pio platform install --with-package framework-arduino atmelavr

ADD buildFirmware.sh /buildFirmware.sh
RUN FIRMWARE_DIR=/WebControl/firmware BUILD_DIR=/tmp/firmware-build bash /buildFirmware.sh \
    && rm -rf /tmp/firmware-build

ADD . /WebControl

# Pre-compile the pyc files (for faster Docker image startup)
RUN python3 -m compileall /WebControl

WORKDIR /WebControl
EXPOSE 5000/tcp
CMD python3 /WebControl/main.py
