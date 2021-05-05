#!/usr/bin/env bash
#
# Diagnostic script for Linux to report information
#

# Kernel
uname -a

# CPU details
cat /proc/cpuinfo

# shellcheck disable=SC2003
if [ "$(expr substr "$(uname -s)" 1 5)" == "Linux" ]; then
  osname="ubuntu"
  osversion="18.04"
  osarch="x64"

  if [ -f /etc/os-release ]; then
    # freedesktop.org and systemd
    . /etc/os-release
    osname=$NAME
    osversion=$VERSION_ID
  elif type lsb_release >/dev/null 2>&1; then
    # linuxbase.org
    osname=$(lsb_release -si)
    osversion=$(lsb_release -sr)
  elif [ -f /etc/lsb-release ]; then
    # For some versions of Debian/Ubuntu without lsb_release command
    . /etc/lsb-release
    osname=$DISTRIB_ID
    osversion=$DISTRIB_RELEASE
  elif [ -f /etc/debian_version ]; then
    # Older Debian/Ubuntu/etc.
    osname=Debian
    osversion=$(cat /etc/debian_version)
  elif [ -f /etc/SuSe-release ]; then
    # Older SuSE/etc.
    echo "Suse?"
  elif [ -f /etc/redhat-release ]; then
    # Older Red Hat, CentOS, etc.
    echo "RedHat?"
  else
    # Fall back to uname, e.g. "Linux <version>", also works for BSD, etc.
    osname=$(uname -s)
    osversion=$(uname -r)
  fi

  osname=$(echo "$osname" | tr '[:upper:]' '[:lower:]' )

  case $(uname -m) in
    x86_64)
      osarch="x64"  # or AMD64 or Intel64 or whatever
      ;;
    i*86)
      osarch="x86"  # or IA32 or Intel32 or whatever
      ;;
    *)
      # leave ARCH as-is
      ;;
  esac
else
  echo "Not a Linux system?"
fi

echo "$osname"
echo "$osversion"
echo $osarch
