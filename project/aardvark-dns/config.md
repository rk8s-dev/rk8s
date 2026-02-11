# Configuration format

Aardvark-dns will read configuration files from a given directory.

Inside this directory there should be at least one config file. The name of the file equals the network name.

### First line
The first line in the config must contain a comma separated list of listening ips for this network, usually the bridge ips.
At least one ip must be given.
**Note**: An optional second column of comma delimited domain name servers can be used at the network level. All containers
on that network will inherit all the specified name servers instead of using the host's resolver.

```
[comma seperated ip4,ipv6 list][(optional)[space][comma seperated DNS servers]]
```

### Container entries
All following lines must contain the dns entries in this format:
```
[containerID][space][comma sparated ipv4 list][space][comma separated ipv6 list][space][comma separated dns names][(optional)[space][comma seperated DNS servers]]
```

Aardvark-dns will reload all config files when receiving a SIGHUB signal.


## Example

```
10.0.0.1,fdfd::1
f35256b5e2f72ec8cb7d974d4f8841686fc8921fdfbc867285b50164e313f715 10.0.0.2 fdfd::2 testmulti1 8.8.8.8,1.1.1.1
e5df0cdbe0136a30cc3e848d495d2cc6dada25b7dedc776b4584ce2cbba6f06f 10.0.0.3 fdfd::3 testmulti2
```
## Example with network scoped DNS servers

```
10.0.0.1,fdfd::1 8.8.8.8,1.1.1.1
f35256b5e2f72ec8cb7d974d4f8841686fc8921fdfbc867285b50164e313f715 10.0.0.2 fdfd::2 testmulti1 8.8.8.8,1.1.1.1
e5df0cdbe0136a30cc3e848d495d2cc6dada25b7dedc776b4584ce2cbba6f06f 10.0.0.3 fdfd::3 testmulti2
```

Also see [./src/test/config/](./src/test/config/) for more config examples
