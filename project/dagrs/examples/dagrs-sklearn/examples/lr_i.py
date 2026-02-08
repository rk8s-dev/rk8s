import numpy as np
from scipy.io import loadmat
from scipy.optimize import minimize
import sys


def load_data(path):
    data=loadmat(path)
    X=data["X"]
    y=data["y"]
    return X,y

def sigmoid(z):
    return 1/(1+np.exp(-z))

def regularized_cost(Theta,X,y,l):
    ThetaReg=Theta[1:]
    cost=(-y*np.log(sigmoid(X@Theta)))-(1-y)*np.log(1-sigmoid((X@Theta)))
    reg=(ThetaReg@ThetaReg)*l/(2*len(X))
    return np.mean(cost)+reg


def regularized_gradient(Theta,X,y,l):
    ThetaReg=Theta[1:]
    cost=(X.T@(sigmoid(X@Theta)-y))*(1/len(X))
    reg=np.concatenate([np.array([0]),(l/len(X))*ThetaReg])
    return cost+reg

def one_vs_all(X,y,l,i):
    Theta=np.zeros(X.shape[1])
    y_i=np.array([1 if labal==(i + 1) else 0 for labal in y])
    ret=minimize(fun=regularized_cost, x0=Theta,args=(X,y_i,l),method='TNC',jac=regularized_gradient)
    return ret.x

argc = len(sys.argv)
argv = sys.argv

X, y = load_data(argv[1])
X=np.insert(X,0,1,axis=1)
y=y.flatten()

i = int(argv[2])
theta = (one_vs_all(X, y, 1, i))

s = np.array2string(theta, max_line_width=1 << 31)
with open(f"theta_{i}.txt", "w") as f:
    f.write(s)
print(f"theta_{i}.txt")