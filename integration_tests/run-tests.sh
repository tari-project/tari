if [ ! -d cucumber_output ]; then
   mkdir cucumber_output
fi
./node_modules/.bin/cucumber-js -f @cucumber/pretty-formatter -f json:cucumber_output/tests.cucumber "$@"
node ./generate_report.js
