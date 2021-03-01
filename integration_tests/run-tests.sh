./node_modules/.bin/cucumber-js -f ./node_modules/cucumber-pretty -f json:temp/output.json "$@"
node ./generate_report.js
