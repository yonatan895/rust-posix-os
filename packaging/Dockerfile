FROM debian:bookworm-slim
RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        qemu-system-x86 ovmf ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /opt/posix-os
COPY image /opt/posix-os/image
COPY run.sh /opt/posix-os/run.sh
RUN chmod +x /opt/posix-os/run.sh
ENV OVMF_PATH=/usr/share/OVMF/OVMF_CODE_4M.fd
EXPOSE 0
CMD ["/opt/posix-os/run.sh"]
