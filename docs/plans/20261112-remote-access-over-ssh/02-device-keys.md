# 02 Device keys

> Goal: after its first login, a device connects with an SSH key of its own and needs no password again.
> Completion criteria: after one login with the user's password, the application installs its own public key for the
> remote user and connects with it from then on; removing that key on the remote machine refuses the device while the
> owner's other devices stay connected; the private key never leaves the device.

## Technical design

- [ ] A key pair made per device and per remote host, the private key never leaving the device.
- [ ] Installing the public key for the remote user, appending to its authorized keys without disturbing other
      entries, and marking it with the device's name so it can be told apart.
- [ ] Switching the remote daemon entry to the device key once it is installed.

## Notes for the developer

- **Development notes**: installing the key is an `ssh-copy-id`-style append over the connection milestone 01 opens;
  the remote user's existing keys and file permissions are left as they were.
