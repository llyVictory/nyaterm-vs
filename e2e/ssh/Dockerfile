FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends openssh-server \
    && rm -rf /var/lib/apt/lists/* && useradd --create-home --shell /bin/bash test \
    && mkdir -p /run/sshd && rm -f /etc/ssh/ssh_host_* \
    && printf 'PasswordAuthentication yes\nPermitRootLogin no\nUsePAM no\nSubsystem sftp internal-sftp\n' > /etc/ssh/sshd_config
COPY start.sh /start.sh
RUN sed -i 's/\r$//' /start.sh && chmod +x /start.sh
VOLUME ["/home/test"]
EXPOSE 22
ENTRYPOINT ["/start.sh"]
