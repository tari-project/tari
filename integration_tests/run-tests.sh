./node_modules/.bin/cucumber-js -f ./node_modules/cucumber-pretty -f json:cucumber_output/tests.cucumber "$@"
node ./generate_report.js
