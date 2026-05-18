#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Created on Wed Sep 16 09:12:39 2020

@author: Philip Robinson
"""
import sys;
import matplotlib.pyplot as plt
import networkx as nx
import glob
from matplotlib import animation

import os;

if len(sys.argv) != 3:
    print("Usage: <Dot file directory> <Output directory>")
    exit(1)

dot_path = sys.argv[1];
output_path = sys.argv[2];

dot_path = "/tmp/memorynet_temp/join_propagation";
output_path = "/tmp/memorynet/join_propagation";

print("Input Dot file directory: ", dot_path);
print("Output directory: ", output_path)

#Clear output directory
if os.path.exists(output_path):
    output_files = glob.glob(os.path.join(output_path,'*'));
    for file in output_files:
        os.remove(file)
else:
    os.makedirs(output_path)

if not os.path.exists(os.path.join(dot_path,'neighbours-000.dot')):
    print("Starting file neighbours-000.dot must exist");
    exit(3)
    

files = glob.glob(os.path.join(dot_path,"hop*"));
files.sort()
if len(files) == 0:
    print("No files to process")
    exit(2) 

# Build plot
fig, ax = plt.subplots(figsize=(15,10))
starting_graph = nx.drawing.nx_pydot.read_dot(os.path.join(dot_path,'neighbours-000.dot'))
starting_graph = nx.MultiDiGraph(starting_graph.to_directed(), directed=True)
pos = nx.spring_layout(starting_graph, k=0.5)
nx.draw_networkx_nodes(starting_graph, pos, node_color="Grey")
nx.draw_networkx_labels(starting_graph, pos, font_weight='bold')
nx.draw_networkx_edges(starting_graph, pos, arrows=True, alpha=0.4)

frame_index = 0;
plt.axis('off')
plt.savefig(output_path + "/{:03d}.png".format(frame_index), bbox_inches='tight', dpi=200)
plt.clf()

old_hops = []
for file in files:
    frame_index
    print("Rendering: ", file)
    filename = file.split('.')[-2].split('/')[-1];
    hop = nx.drawing.nx_pydot.read_dot(file)
    
    nx.draw_networkx_nodes(starting_graph, pos, node_color="Grey")
    nx.draw_networkx_labels(starting_graph, pos, font_weight='bold')
    nx.draw_networkx_edges(starting_graph, pos, arrows=True, alpha=0.4)

    for h in old_hops:
            nx.draw_networkx_edges(h, pos, arrows=True, edge_color="Red", width=3, alpha= 0.2)        


    nx.draw_networkx_edges(hop, pos, arrows=True, edge_color="Red", width=2)

    old_hops.append(hop)

    plt.axis('off')
    plt.savefig(output_path + "/" + filename + ".png", bbox_inches='tight', dpi=200)
    plt.clf()

exit(0)

