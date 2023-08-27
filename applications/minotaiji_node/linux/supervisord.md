# Install taiji_base_node in daemon mode - supervisord

Initial test with root user and tor setup as a services and running. 
Need to look into non root user setups in future. 

Install supervisor for your platform, Ubuntu 16.04 to 20.04 should be as easy as
```
sudo apt-get install supervisor
```

Make folders
```
sudo mkdir /usr/local/taiji
cd /usr/local/taiji
sudo mkdir bak bin dists log log/rolled screen-logs
```
Copy ```minotaiji_node``` binary into the above folder, either from the build folder or from the downloaded archive, which is extracted 
```
sudo cp -v /home/vagrant/src/taiji/target/release/minotaiji_node  /usr/local/taiji/bin
```
Create your minotaiji_node configs
```
sudo /usr/local/taiji/bin/taiji_base_node --base-path /usr/local/taiji --init 
```
Setup ```minotaiji_node``` services in supervisord -
```/etc/supervisor/conf.d/taiji_base_node.conf```
Run the following command
```
cat << EOD | sudo tee -a  /etc/supervisor/conf.d/taiji_base_node.conf
[program:taiji_base_node]
process_name=taiji_base_node
command=/usr/local/taiji/bin/taiji_base_node --daemon-mode --base-path /usr/local/taiji --config config.toml --log-config log4rs.yml
directory=/usr/local/taiji/
autostart=true
autorestart=true
stderr_logfile=/usr/local/taiji/log/taiji_base_node.err.log
stdout_logfile=/usr/local/taiji/log/taiji_base_node.out.log

EOD
```
Update supervisord and start the ```minotaiji_node```
```
sudo supervisorctl reread
sudo supervisorctl update
sudo supervisorctl start taiji_base_node
```
Can also restart supervisord
```
sudo service supervisor restart
```

