FROM alpine:3.19

# Install OpenSSH server and utilities
RUN apk add --no-cache openssh-server bash netcat-openbsd

# Create test user with known password
RUN adduser -D -s /bin/bash testuser && \
    echo "testuser:testpassword" | chpasswd

# Setup SSH directories with correct permissions
RUN mkdir -p /home/testuser/.ssh && \
    chown -R testuser:testuser /home/testuser/.ssh && \
    chmod 700 /home/testuser/.ssh

# Copy custom sshd_config
COPY sshd_config /etc/ssh/sshd_config

# Generate host keys
RUN ssh-keygen -A

EXPOSE 22

# Run sshd in foreground with debug logging
CMD ["/usr/sbin/sshd", "-D", "-e"]
