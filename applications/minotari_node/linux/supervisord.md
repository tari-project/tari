# Install minotari_node in daemon mode - supervisord

Initial test with root user and tor setup as a services and running. 
Need to look into non root user setups in future. 

Install supervisor for your platform, Ubuntu 16.04 to 20.04 should be as easy as
```
sudo apt-get install supervisor
```

Make folders
```
sudo mkdir /usr/local/tari
cd /usr/local/tari
sudo mkdir bak bin dists log log/rolled screen-logs
```
Copy ```minotari_node``` binary into the above folder, either from the build folder or from the downloaded archive, which is extracted 
```
sudo cp -v /home/vagrant/src/tari/target/release/minotari_node  /usr/local/tari/bin
```
Create your minotari_node configs
```
sudo /usr/local/tari/bin/minotari_node --base-path /usr/local/tari --init 
```
Setup ```minotari_node``` services in supervisord -
```/etc/supervisor/conf.d/minotari_node.conf```
Run the following command
```
cat << EOD | sudo tee -a  /etc/supervisor/conf.d/minotari_node.conf
[program:minotari_node]
process_name=minotari_node
command=/usr/local/tari/bin/minotari_node --daemon-mode --base-path /usr/local/tari --config config.toml --log-config log4rs.yml
directory=/usr/local/tari/
autostart=true
autorestart=true
stderr_logfile=/usr/local/tari/log/minotari_node.err.log
stdout_logfile=/usr/local/tari/log/minotari_node.out.log

EOD
```
Update supervisord and start the ```minotari_node```
```
sudo supervisorctl reread
sudo supervisorctl update
sudo supervisorctl start minotari_node
```
Can also restart supervisord
```
sudo service supervisor restart
```

