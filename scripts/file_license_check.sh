#!/usr/bin/env bash
#
# Must be run from the repo root
#

#set -xo pipefail
set -e

check_for() {
  if prog_location=$(which ${1}) ; then
    if result=$(${1} --version 2>/dev/null); then
      result="${1}: ${result} INSTALLED ✓"
    else
      result="${1}: INSTALLED ✓"
    fi
  else
    result="${1}: MISSING ⨯"
  fi
}

check_requirements() {
  echo "List of requirements and possible test:"
  req_progs=(
    mktemp
    rg
    diff
  )
  for RProg in "${req_progs[@]}"; do
    check_for ${RProg}
    echo "${result}"
    if [[ "${result}" == "${RProg}: MISSING ⨯" ]]; then
      echo "!! Install ${RProg} and try again !!"
      exit -1
    fi
  done

  if [ ! -f .license.ignore ]; then
    echo "!! No .license.ignore file !!"
    exit -1
  fi
}

check_requirements

diffparms=${diffparms:-"-u --suppress-blank-empty --strip-trailing-cr --color=never"}
rgTemp=${rgTemp:-$(mktemp)}

# rg -i "Copyright.*The Tari Project" --files-without-match \
#    -g '!*.{Dockerfile,asc,bat,config,config.js,css,csv,drawio,env,gitkeep,hbs,html,ini,iss,json,lock,md,min.js,ps1,py,rc,scss,sh,sql,svg,toml,txt,yml,vue}' . \
#    | sort > /tmp/rgtemp

# Exclude files without extensions as well as those with extensions that are not in the list
#
rg -i "Copyright.*The Tari Project" --files-without-match \
    -g '!*.{Dockerfile,asc,bat,config,config.js,css,csv,drawio,env,gitkeep,hbs,html,ini,iss,json,lock,md,min.js,ps1,py,rc,scss,sh,sql,svg,toml,txt,yml,vue,liquid,otf,d.ts,mjs}' . \
    | while IFS= read -r file; do
        if [[ -n $(basename "$file" | grep -E '\.') ]]; then
            echo "$file"
        fi
    done | sort > ${rgTemp}

# Sort the .license.ignore file as sorting seems to behave differently on different platforms
licenseIgnoreTemp=${licenseIgnoreTemp:-$(mktemp)}
cat .license.ignore | sort > ${licenseIgnoreTemp}

DIFFS=$( diff ${diffparms} ${licenseIgnoreTemp} ${rgTemp} || true )

# clean up
rm -vf ${rgTemp}
rm -vf ${licenseIgnoreTemp}

if [ -n "${DIFFS}" ]; then
    echo "New files detected that either need copyright/license identifiers added, "
    echo "or they need to be added to .license.ignore"
    echo "NB: The ignore file must be sorted alphabetically!"

    echo "Diff:"
    echo "${DIFFS}"
    exit 1
else
    exit 0
fi
